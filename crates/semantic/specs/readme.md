## Design priorities.

- Great AI support -> can provide sufficient context to the agent to answer the user question.
- Have sufficient information to generate the SQL through a semantic query engine.
- Declerative
- Flexible and easy to use.
- Compatible with other semantic layers like Omni, Cube.js, AtScale, etc.. because we may need to ingest these semantic layers to our format or convert our semantic layer to theirs- Universal semantics layers ?

## Concepts

- **View**: Represents a view on data. A model can be based on a table, a SQL query or any other kind of data that we want to support later. Defined in .view.yaml files.
- **Entity**: Similar to entities in dbt MetricFlow, representing a distinct object or concept within the data. Entities enable automatic relationship discovery and intelligent joins between views.
- **Dimension**: Represents attributes of an entity, such as user properties or order details. Defined inside view files.
- **Measure**: Represents aggregations or calculations based on dimensions, such as total sales or average order value. Defined inside view files.
- **Topic**: Represents a collection of related views, entities, and metrics. Topics help organize the semantic layer and provide a high-level structure for data exploration. Defined in .topic.yaml files.

## Project structure

```
topics/
├── sales.topic.yaml
├── finance.topic.yaml
views/
├── customers.view.yaml
├── products.view.yaml
├── orders.view.yaml
```
