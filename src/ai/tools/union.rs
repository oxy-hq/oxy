#[macro_export]
macro_rules! union_tools {
    ($union_tool:ident, $input:ident, $($tool:ident, $tool_input:ident),+) => {
        #[derive(Deserialize, JsonSchema)]
        #[serde(untagged)]
        pub enum $input {
            $($tool_input($tool_input)),+
        }

        $(
            impl From<$tool_input> for $input {
                fn from(tool: $tool_input) -> Self {
                    $input::$tool_input(tool)
                }
            }
        )+

        $(
            impl TryInto<$tool_input> for $input {
                type Error = anyhow::Error;
                fn try_into(self) -> anyhow::Result<$tool_input> {
                    if let $input::$tool_input(t) = self {
                        Ok(t)
                    } else {
                        anyhow::bail!(OnyxError::RuntimeError("Could not convert".to_string()))
                    }
                }
            }
        )+


        #[derive(Debug)]
        pub enum $union_tool {
            $($tool($tool)),+
        }

        $(
            impl From<$tool> for $union_tool {
                fn from(tool: $tool) -> Self {
                    $union_tool::$tool(tool)
                }
            }
        )+

        #[async_trait]
        impl Tool for $union_tool {
            type Input = $input;

            fn param_spec(&self) -> anyhow::Result<serde_json::Value> {
                match self {
                    $($union_tool::$tool(t) => {
                            t.param_spec()
                        }
                    ),+
                }
            }

            async fn call(&self, parameters: &str) -> anyhow::Result<String> {
                match self {
                    $($union_tool::$tool(t) => t.call(parameters).await.map_err(|e| e.into())),+
                }
            }

            async fn call_internal(&self, parameters: &Self::Input) -> anyhow::Result<String> {
                match (self, parameters) {
                    $(($union_tool::$tool(t), $input::$tool_input(i)) => {
                            t.call_internal(i).await
                        }
                    ),+
                    _ => anyhow::bail!(OnyxError::RuntimeError("Could not convert".to_string()))
                }
            }

            /// Returns the `ToolDescription` containing metadata about the tool.
            fn name(&self) -> String {
              match self {
                  $($union_tool::$tool(t) => t.name()),+
              }
            }

            /// Returns the `ToolDescription` containing metadata about the tool.
            fn description(&self) -> String {
                match self {
                    $($union_tool::$tool(t) => t.description()),+
                }
            }
        }
    };
}
