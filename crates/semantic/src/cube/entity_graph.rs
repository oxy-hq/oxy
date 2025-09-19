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
                        let join_condition = format!(
                            "{{{}.{}}} = {{{}.{}}}",
                            foreign_view, foreign_ent.key, primary_view, primary_ent.key
                        );

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

            // Create CubeJS join definition
            let cube_join = CubeJoin {
                name: target_view.clone(),
                sql: join.on_condition.clone(),
                relationship: match join.relationship_type {
                    RelationshipType::OneToOne => "one_to_one".to_string(),
                    RelationshipType::OneToMany => "one_to_many".to_string(),
                    RelationshipType::ManyToOne => "many_to_one".to_string(),
                    RelationshipType::ManyToMany => "many_to_many".to_string(),
                },
            };

            joins.push(cube_join);
        }

        joins
    }
}
