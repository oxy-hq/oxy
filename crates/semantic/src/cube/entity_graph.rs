use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{EntityType, SemanticLayer, errors::SemanticLayerError};

use super::models::CubeJoin;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinRelationship {
    pub from_view: String,
    pub to_view: String,
    pub join_type: JoinType,
    pub on_condition: String,
    pub relationship_type: RelationshipType,
}

/// Types of joins supported
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinType {
    LeftJoin,
    RightJoin,
    InnerJoin,
    FullOuterJoin,
}

/// Types of relationships between entities
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipType {
    OneToOne,
    OneToMany,
    ManyToOne,
    ManyToMany,
}

impl JoinType {
    pub fn to_string(&self) -> String {
        match self {
            JoinType::LeftJoin => "LEFT JOIN".to_string(),
            JoinType::RightJoin => "RIGHT JOIN".to_string(),
            JoinType::InnerJoin => "INNER JOIN".to_string(),
            JoinType::FullOuterJoin => "FULL OUTER JOIN".to_string(),
        }
    }
}

/// Represents the entity graph for automatic join resolution
///
/// The EntityGraph analyzes the semantic layer to identify relationships between views
/// based on shared entities. It automatically generates join relationships that can be
/// used by query engines like CubeJS to perform intelligent cross-view queries.
///
/// # How it works:
/// 1. Scans all views in the semantic layer for entities
/// 2. Maps primary entities to their owning views  
/// 3. Maps foreign entities to views that reference them
/// 4. Generates join relationships between views that share entities
/// 5. Creates appropriate join conditions using entity expressions
///
/// # Example:
/// If you have:
/// - `customers` view with primary entity `customer` (expr: `customer_id`)
/// - `orders` view with foreign entity `customer` (expr: `customer_id`)
///
/// The EntityGraph will generate a join relationship:
/// ```sql
/// ${orders.customer_id} = ${customers.customer_id}
/// ```
#[derive(Debug, Clone)]
pub struct EntityGraph {
    /// Map of entity name to views that contain this entity as primary
    pub(crate) primary_entities: HashMap<String, String>,
    /// Map of entity name to views that contain this entity as foreign
    pub(crate) foreign_entities: HashMap<String, Vec<String>>,
    /// Generated join relationships between views
    pub(crate) joins: Vec<JoinRelationship>,
}

impl EntityGraph {
    /// Create a new entity graph from a semantic layer
    pub fn from_semantic_layer(semantic_layer: &SemanticLayer) -> Result<Self, SemanticLayerError> {
        let mut primary_entities = HashMap::new();
        let mut foreign_entities: HashMap<String, Vec<String>> = HashMap::new();
        let mut joins = Vec::new();

        // First pass: collect primary and foreign entities
        for view in &semantic_layer.views {
            for entity in &view.entities {
                match entity.entity_type {
                    EntityType::Primary => {
                        if primary_entities.contains_key(&entity.name) {
                            return Err(SemanticLayerError::ConfigurationError(format!(
                                "Duplicate primary entity '{}' found in view '{}'. Primary entity already defined in view '{}'",
                                entity.name, view.name, primary_entities[&entity.name]
                            )));
                        }
                        primary_entities.insert(entity.name.clone(), view.name.clone());
                    }
                    EntityType::Foreign => {
                        foreign_entities
                            .entry(entity.name.clone())
                            .or_default()
                            .push(view.name.clone());
                    }
                }
            }
        }

        // Second pass: generate joins based on shared entities
        for (entity_name, views_with_foreign_entity) in &foreign_entities {
            if let Some(primary_view) = primary_entities.get(entity_name) {
                for foreign_view in views_with_foreign_entity {
                    // Find the actual entity objects to build the join condition
                    let primary_entity = semantic_layer
                        .views
                        .iter()
                        .find(|v| &v.name == primary_view)
                        .and_then(|v| {
                            v.entities.iter().find(|e| {
                                &e.name == entity_name && e.entity_type == EntityType::Primary
                            })
                        });

                    let foreign_entity = semantic_layer
                        .views
                        .iter()
                        .find(|v| &v.name == foreign_view)
                        .and_then(|v| {
                            v.entities.iter().find(|e| {
                                &e.name == entity_name && e.entity_type == EntityType::Foreign
                            })
                        });

                    if let (Some(primary_ent), Some(foreign_ent)) = (primary_entity, foreign_entity)
                    {
                        // Get keys from both entities
                        let primary_keys = primary_ent.get_keys();
                        let foreign_keys = foreign_ent.get_keys();

                        // Validate that both entities have the same number of keys
                        if primary_keys.len() != foreign_keys.len() {
                            return Err(SemanticLayerError::ConfigurationError(format!(
                                "Entity '{}' has mismatched key counts: primary entity in view '{}' has {} key(s), foreign entity in view '{}' has {} key(s)",
                                entity_name,
                                primary_view,
                                primary_keys.len(),
                                foreign_view,
                                foreign_keys.len()
                            )));
                        }

                        // Build join condition for single or composite keys
                        let join_condition = if primary_keys.len() == 1 {
                            // Simple single-key join
                            format!(
                                "{{{}.{}}} = {{{}.{}}}",
                                foreign_view, foreign_keys[0], primary_view, primary_keys[0]
                            )
                        } else {
                            // Composite key join with AND conditions
                            primary_keys
                                .iter()
                                .zip(foreign_keys.iter())
                                .map(|(pk, fk)| {
                                    format!(
                                        "{{{}.{}}} = {{{}.{}}}",
                                        foreign_view, fk, primary_view, pk
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join(" AND ")
                        };

                        let join = JoinRelationship {
                            from_view: foreign_view.clone(),
                            to_view: primary_view.clone(),
                            join_type: JoinType::LeftJoin, // Default to left join
                            on_condition: join_condition,
                            relationship_type: RelationshipType::ManyToOne, // Foreign to primary is typically many-to-one
                        };

                        joins.push(join);
                    }
                }
            }
        }

        Ok(EntityGraph {
            primary_entities,
            foreign_entities,
            joins,
        })
    }

    /// Get all joins for the entity graph
    pub fn get_joins(&self) -> &[JoinRelationship] {
        &self.joins
    }

    /// Get primary entities map
    pub fn get_primary_entities(&self) -> &HashMap<String, String> {
        &self.primary_entities
    }

    /// Get foreign entities map  
    pub fn get_foreign_entities(&self) -> &HashMap<String, Vec<String>> {
        &self.foreign_entities
    }

    /// Find join path between two views using graph traversal
    ///
    /// Uses breadth-first search to find the shortest path between two views.
    /// Returns a vector of join relationships that form the path from source to target.
    pub fn find_join_path(&self, from_view: &str, to_view: &str) -> Option<Vec<&JoinRelationship>> {
        use std::collections::{HashMap, HashSet, VecDeque};

        if from_view == to_view {
            return Some(vec![]);
        }

        // Build adjacency list for graph traversal
        let mut graph: HashMap<&str, Vec<&JoinRelationship>> = HashMap::new();

        for join in &self.joins {
            // Add bidirectional edges since joins can be traversed in both directions
            graph.entry(join.from_view.as_str()).or_default().push(join);
            graph.entry(join.to_view.as_str()).or_default().push(join);
        }

        // BFS to find shortest path
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        let mut parent: HashMap<&str, (&str, &JoinRelationship)> = HashMap::new();

        queue.push_back(from_view);
        visited.insert(from_view);

        while let Some(current_view) = queue.pop_front() {
            if current_view == to_view {
                // Reconstruct path by backtracking through parents
                let mut path = Vec::new();
                let mut current = to_view;

                while let Some((prev_view, join)) = parent.get(current) {
                    path.push(*join);
                    current = prev_view;
                }

                path.reverse();
                return Some(path);
            }

            // Explore neighbors
            if let Some(joins) = graph.get(current_view) {
                for join in joins {
                    let next_view = if join.from_view == current_view {
                        join.to_view.as_str()
                    } else {
                        join.from_view.as_str()
                    };

                    if !visited.contains(next_view) {
                        visited.insert(next_view);
                        parent.insert(next_view, (current_view, join));
                        queue.push_back(next_view);
                    }
                }
            }
        }

        None // No path found
    }

    /// Validates that all views in a topic are reachable from the base view
    ///
    /// Returns a vector of unreachable view names if any views cannot be reached,
    /// or an empty vector if all views are reachable.
    pub fn validate_base_view_reachability(
        &self,
        base_view: &str,
        topic_views: &[String],
    ) -> Vec<String> {
        let mut unreachable_views = Vec::new();

        for view in topic_views {
            // Skip the base view itself
            if view == base_view {
                continue;
            }

            // Check if there's a path from base_view to this view
            if self.find_join_path(base_view, view).is_none() {
                unreachable_views.push(view.clone());
            }
        }

        unreachable_views
    }

    /// Get joins for a specific view
    pub fn get_joins_for_view(&self, view_name: &str) -> Vec<&JoinRelationship> {
        self.joins
            .iter()
            .filter(|join| join.from_view == view_name || join.to_view == view_name)
            .collect()
    }

    /// Generate CubeJS joins for a specific view
    pub fn generate_cube_joins(&self, view_name: &str) -> Vec<CubeJoin> {
        let mut joins = Vec::new();

        for join in self.get_joins_for_view(view_name) {
            // Determine which view this cube should join to
            let target_view = if join.from_view == view_name {
                &join.to_view
            } else {
                &join.from_view
            };

            // Determine the relationship type from the perspective of the current view
            // If the join direction is reversed, we need to flip the relationship
            let relationship = if join.from_view == view_name {
                // We're joining in the stored direction
                match join.relationship_type {
                    RelationshipType::OneToOne => "one_to_one".to_string(),
                    RelationshipType::OneToMany => "one_to_many".to_string(),
                    RelationshipType::ManyToOne => "many_to_one".to_string(),
                    RelationshipType::ManyToMany => "many_to_many".to_string(),
                }
            } else {
                // We're joining in the reverse direction, flip the relationship
                match join.relationship_type {
                    RelationshipType::OneToOne => "one_to_one".to_string(), // Symmetric
                    RelationshipType::OneToMany => "many_to_one".to_string(), // Flip
                    RelationshipType::ManyToOne => "one_to_many".to_string(), // Flip
                    RelationshipType::ManyToMany => "many_to_many".to_string(), // Symmetric
                }
            };

            // Create CubeJS join definition
            let cube_join = CubeJoin {
                name: target_view.clone(),
                sql: join.on_condition.clone(),
                relationship,
            };

            joins.push(cube_join);
        }

        joins
    }

    /// Get dependency graph for incremental builds
    ///
    /// Returns a BTreeMap where:
    /// - Key: view name
    /// - Value: vector of view names that this view depends on
    ///
    /// A view depends on another view if it has a foreign entity that references
    /// the other view's primary entity.
    ///
    /// Note: Uses BTreeMap for stable iteration order (sorted keys).
    ///
    /// # Example
    /// If `orders` view has a foreign entity `customer` that references
    /// `customers` view's primary entity, then:
    /// - `orders` depends on `customers`
    /// - The map will contain: `{"orders": ["customers"]}`
    pub fn get_dependency_graph(&self) -> std::collections::BTreeMap<String, Vec<String>> {
        let mut graph: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();

        // For each join, the from_view depends on the to_view
        // (foreign view depends on primary view)
        for join in &self.joins {
            graph
                .entry(join.from_view.clone())
                .or_default()
                .push(join.to_view.clone());
        }

        graph
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Entity, EntityType, SemanticLayer, View};

    #[test]
    fn test_composite_key_join_generation() {
        // Create a view with composite primary key
        let order_items_view = View {
            name: "order_items".to_string(),
            description: "Order line items".to_string(),
            table: Some("order_items".to_string()),
            sql: None,
            datasource: Some("test_db".to_string()),
            label: None,
            entities: vec![Entity {
                name: "order_item".to_string(),
                entity_type: EntityType::Primary,
                description: "Order item entity".to_string(),
                key: None,
                keys: Some(vec!["order_id".to_string(), "line_item_id".to_string()]),
            }],
            dimensions: vec![],
            measures: None,
        };

        // Create a view with composite foreign key
        let shipments_view = View {
            name: "shipments".to_string(),
            description: "Order shipments".to_string(),
            table: Some("shipments".to_string()),
            sql: None,
            datasource: Some("test_db".to_string()),
            label: None,
            entities: vec![
                Entity {
                    name: "shipment".to_string(),
                    entity_type: EntityType::Primary,
                    description: "Shipment entity".to_string(),
                    key: Some("shipment_id".to_string()),
                    keys: None,
                },
                Entity {
                    name: "order_item".to_string(),
                    entity_type: EntityType::Foreign,
                    description: "Order item being shipped".to_string(),
                    key: None,
                    keys: Some(vec!["order_id".to_string(), "line_item_id".to_string()]),
                },
            ],
            dimensions: vec![],
            measures: None,
        };

        let semantic_layer = SemanticLayer {
            views: vec![order_items_view, shipments_view],
            topics: None,
            metadata: None,
        };

        let entity_graph = EntityGraph::from_semantic_layer(&semantic_layer).unwrap();

        // Check that a join was generated
        assert_eq!(entity_graph.joins.len(), 1);

        let join = &entity_graph.joins[0];
        assert_eq!(join.from_view, "shipments");
        assert_eq!(join.to_view, "order_items");

        // Check that the join condition includes both keys with AND
        assert!(join.on_condition.contains("order_id"));
        assert!(join.on_condition.contains("line_item_id"));
        assert!(join.on_condition.contains(" AND "));

        // Check the exact format
        assert_eq!(
            join.on_condition,
            "{shipments.order_id} = {order_items.order_id} AND {shipments.line_item_id} = {order_items.line_item_id}"
        );
    }

    #[test]
    fn test_mismatched_composite_key_counts() {
        // Create a view with 2-column composite key
        let view1 = View {
            name: "view1".to_string(),
            description: "View 1".to_string(),
            table: Some("view1".to_string()),
            sql: None,
            datasource: Some("test_db".to_string()),
            label: None,
            entities: vec![Entity {
                name: "entity1".to_string(),
                entity_type: EntityType::Primary,
                description: "Entity with 2 keys".to_string(),
                key: None,
                keys: Some(vec!["key1".to_string(), "key2".to_string()]),
            }],
            dimensions: vec![],
            measures: None,
        };

        // Create a view with 3-column composite key (mismatch)
        let view2 = View {
            name: "view2".to_string(),
            description: "View 2".to_string(),
            table: Some("view2".to_string()),
            sql: None,
            datasource: Some("test_db".to_string()),
            label: None,
            entities: vec![Entity {
                name: "entity1".to_string(),
                entity_type: EntityType::Foreign,
                description: "Entity with 3 keys".to_string(),
                key: None,
                keys: Some(vec![
                    "key1".to_string(),
                    "key2".to_string(),
                    "key3".to_string(),
                ]),
            }],
            dimensions: vec![],
            measures: None,
        };

        let semantic_layer = SemanticLayer {
            views: vec![view1, view2],
            topics: None,
            metadata: None,
        };

        // This should return an error due to mismatched key counts
        let result = EntityGraph::from_semantic_layer(&semantic_layer);
        assert!(result.is_err());

        if let Err(err) = result {
            let err_msg = format!("{:?}", err);
            assert!(err_msg.contains("mismatched key counts"));
        }
    }

    #[test]
    fn test_single_key_still_works() {
        // Test that single-key entities still work correctly
        let customers_view = View {
            name: "customers".to_string(),
            description: "Customers".to_string(),
            table: Some("customers".to_string()),
            sql: None,
            datasource: Some("test_db".to_string()),
            label: None,
            entities: vec![Entity {
                name: "customer".to_string(),
                entity_type: EntityType::Primary,
                description: "Customer entity".to_string(),
                key: Some("customer_id".to_string()),
                keys: None,
            }],
            dimensions: vec![],
            measures: None,
        };

        let orders_view = View {
            name: "orders".to_string(),
            description: "Orders".to_string(),
            table: Some("orders".to_string()),
            sql: None,
            datasource: Some("test_db".to_string()),
            label: None,
            entities: vec![Entity {
                name: "customer".to_string(),
                entity_type: EntityType::Foreign,
                description: "Customer who placed order".to_string(),
                key: Some("customer_id".to_string()),
                keys: None,
            }],
            dimensions: vec![],
            measures: None,
        };

        let semantic_layer = SemanticLayer {
            views: vec![customers_view, orders_view],
            topics: None,
            metadata: None,
        };

        let entity_graph = EntityGraph::from_semantic_layer(&semantic_layer).unwrap();

        // Check that a join was generated
        assert_eq!(entity_graph.joins.len(), 1);

        let join = &entity_graph.joins[0];
        assert_eq!(
            join.on_condition,
            "{orders.customer_id} = {customers.customer_id}"
        );
    }
}
