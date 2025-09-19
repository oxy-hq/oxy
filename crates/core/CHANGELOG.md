# Changelog

## [0.1.19](https://github.com/oxy-hq/oxy-internal/compare/0.1.18...0.1.19) (2025-04-18)


### <!-- 1 -->🐛 Bug Fixes

* fallback to single sql type when cache key found ([#641](https://github.com/oxy-hq/oxy-internal/issues/641)) ([7b281f6](https://github.com/oxy-hq/oxy-internal/commit/7b281f69cd694d8841596e53283b951eb249e452))

## [0.1.18](https://github.com/oxy-hq/oxy-internal/compare/0.1.17...0.1.18) (2025-04-18)


### <!-- 1 -->🐛 Bug Fixes

* improve visualize tool output ([#638](https://github.com/oxy-hq/oxy-internal/issues/638)) ([3eecb45](https://github.com/oxy-hq/oxy-internal/commit/3eecb4574e68336fe3abe0850ac5625ed5cdeda2))

## [0.1.17](https://github.com/oxy-hq/oxy-internal/compare/0.1.16...0.1.17) (2025-04-17)


### <!-- 1 -->🐛 Bug Fixes

* update MCP server log message to include localhost URL ([99c1a45](https://github.com/oxy-hq/oxy-internal/commit/99c1a45b9227ae92082b20e3ba6fc669fe8b9b59))

## [0.1.16](https://github.com/oxy-hq/oxy-internal/compare/0.1.15...0.1.16) (2025-04-17)


### <!-- 1 -->🐛 Bug Fixes

* force using ring as default for rustls ([53880db](https://github.com/oxy-hq/oxy-internal/commit/53880dbfb99ef0f0f880e3da130f3f208ba3aa80))

## [0.1.15](https://github.com/oxy-hq/oxy-internal/compare/0.1.14...0.1.15) (2025-04-17)


### <!-- 0 -->🚀 Features

* dockerize oxy and enable postgres as a data backend ([#632](https://github.com/oxy-hq/oxy-internal/issues/632)) ([5f81c3a](https://github.com/oxy-hq/oxy-internal/commit/5f81c3a692239efa57f390b01e3b4c6f1bdbe4fc))


### <!-- 1 -->🐛 Bug Fixes

* always set current dir to project path for mcp stdio, force web server to be started in project path ([#634](https://github.com/oxy-hq/oxy-internal/issues/634)) ([7d1d632](https://github.com/oxy-hq/oxy-internal/commit/7d1d63285a6900731e4537c19c1b50f3a396adf2))

## [0.1.14](https://github.com/oxy-hq/oxy-internal/compare/0.1.13...0.1.14) (2025-04-17)


### <!-- 0 -->🚀 Features

* allow agents to execute workflows ([#618](https://github.com/oxy-hq/oxy-internal/issues/618)) ([cca67ec](https://github.com/oxy-hq/oxy-internal/commit/cca67ec2dc43bb9532977c69e5b73ec29d289171))
* closes [#630](https://github.com/oxy-hq/oxy-internal/issues/630) Improve agent loading state ([#631](https://github.com/oxy-hq/oxy-internal/issues/631)) ([85c20d9](https://github.com/oxy-hq/oxy-internal/commit/85c20d9ea0c074521bd09a4146007fbfa8d14590))
* handle bigquery source directly and increase timeout ([#622](https://github.com/oxy-hq/oxy-internal/issues/622)) ([503ed5c](https://github.com/oxy-hq/oxy-internal/commit/503ed5c9ae1c94c786b94da6aa75f390f3bad915))
* implement stream retry ([#629](https://github.com/oxy-hq/oxy-internal/issues/629)) ([122e57f](https://github.com/oxy-hq/oxy-internal/commit/122e57f519233c0997ba2cea610a49b67cc22c22))


### <!-- 1 -->🐛 Bug Fixes

* add structured response description ([#628](https://github.com/oxy-hq/oxy-internal/issues/628)) ([e8407b5](https://github.com/oxy-hq/oxy-internal/commit/e8407b50f1943da93cdfaa8aeb135ce97c8209d5))
* mcp sse not working ([c4e1516](https://github.com/oxy-hq/oxy-internal/commit/c4e15165bf9837a063af8d07fc57a54662da107e))
* using api_key in azure ([#627](https://github.com/oxy-hq/oxy-internal/issues/627)) ([49d3ca9](https://github.com/oxy-hq/oxy-internal/commit/49d3ca9dd2011194844faf490c30afd0be4581c7))


### <!-- 7 -->⚙️ Miscellaneous Tasks

* release 0.1.14 ([8b6cf7f](https://github.com/oxy-hq/oxy-internal/commit/8b6cf7fdb83ccaad5dbda51bec5764073736d45d))

## [0.1.13](https://github.com/oxy-hq/oxy-internal/compare/0.1.12...0.1.13) (2025-04-15)


### <!-- 0 -->🚀 Features

* [#573](https://github.com/oxy-hq/oxy-internal/issues/573) Visualization meta issue ([#590](https://github.com/oxy-hq/oxy-internal/issues/590)) ([d22479c](https://github.com/oxy-hq/oxy-internal/commit/d22479c6c1af739997b93a15854968ea8e0eaa20))

## [0.1.12](https://github.com/oxy-hq/oxy-internal/compare/0.1.11...0.1.12) (2025-04-14)


### <!-- 0 -->🚀 Features

* support multiple embed documents for a sql file ([#605](https://github.com/oxy-hq/oxy-internal/issues/605)) ([0746c15](https://github.com/oxy-hq/oxy-internal/commit/0746c15ba48689da8b8c63785929733627032bc8))

## [0.1.11](https://github.com/oxy-hq/oxy-internal/compare/0.1.10...0.1.11) (2025-04-11)


### <!-- 0 -->🚀 Features

* add Anthropic/Claude model support ([#582](https://github.com/oxy-hq/oxy-internal/issues/582)) ([8424d64](https://github.com/oxy-hq/oxy-internal/commit/8424d64e2238851cdf4cae559b67b568500872ad))
* add more information to build ([354e85b](https://github.com/oxy-hq/oxy-internal/commit/354e85b65bac27d49caf0f090e72a10f0e75e2ae))
* add openapi docs for customer facing API ([#589](https://github.com/oxy-hq/oxy-internal/issues/589)) ([25a128d](https://github.com/oxy-hq/oxy-internal/commit/25a128dc374979ddc22566fe1e36a88219655868))
* improve embedding ([#593](https://github.com/oxy-hq/oxy-internal/issues/593)) ([d429df0](https://github.com/oxy-hq/oxy-internal/commit/d429df0f1cb54d80658e82fa11a41753ebffe364))

## [0.1.10](https://github.com/oxy-hq/oxy-internal/compare/0.1.9...0.1.10) (2025-04-08)


### <!-- 0 -->🚀 Features

* [#524](https://github.com/oxy-hq/oxy-internal/issues/524) Build testing UI ([#556](https://github.com/oxy-hq/oxy-internal/issues/556)) ([8c119f0](https://github.com/oxy-hq/oxy-internal/commit/8c119f087f2392f95243a571dc1e3b7306d03350))


### <!-- 1 -->🐛 Bug Fixes

* oxy panic and exit silently, load .env from project path when running mcp ([#584](https://github.com/oxy-hq/oxy-internal/issues/584)) ([7289c17](https://github.com/oxy-hq/oxy-internal/commit/7289c1747dc1204894b49fbb0736eb277f286bc0))

## [0.1.9](https://github.com/oxy-hq/oxy-internal/compare/0.1.8...0.1.9) (2025-04-04)


### <!-- 0 -->🚀 Features

* refactor & stabilize mcp server ([#560](https://github.com/oxy-hq/oxy-internal/issues/560)) ([55962a2](https://github.com/oxy-hq/oxy-internal/commit/55962a2c6ad1469116ac645f116998c6d4cffcad))


### <!-- 1 -->🐛 Bug Fixes

* agent datetime awareness ([#558](https://github.com/oxy-hq/oxy-internal/issues/558)) ([0030e66](https://github.com/oxy-hq/oxy-internal/commit/0030e66c25c262443af77a3110de7eaf8d118524))
* context value serialization fail ([#566](https://github.com/oxy-hq/oxy-internal/issues/566)) ([539a8c9](https://github.com/oxy-hq/oxy-internal/commit/539a8c985c99dfc8451f6aea39b14acb554800b9))

## [0.1.8](https://github.com/oxy-hq/oxy-internal/compare/0.1.7...0.1.8) (2025-04-02)


### <!-- 0 -->🚀 Features

* [#518](https://github.com/oxy-hq/oxy-internal/issues/518) implement if else logic for our yaml ([#535](https://github.com/oxy-hq/oxy-internal/issues/535)) ([6aca561](https://github.com/oxy-hq/oxy-internal/commit/6aca561e678be726609021bfa0d9ed384cff1121))
* [#550](https://github.com/oxy-hq/oxy-internal/issues/550) support Jinja filters and data transformation for referenced variable ([#554](https://github.com/oxy-hq/oxy-internal/issues/554)) ([34f424b](https://github.com/oxy-hq/oxy-internal/commit/34f424ba9bbc3df5f5bc56540191ad8227b636b7))
* **beta:** mcp server and improvements to web server ([#553](https://github.com/oxy-hq/oxy-internal/issues/553)) ([5c2aab2](https://github.com/oxy-hq/oxy-internal/commit/5c2aab2299f74d4c1076b7d08e2c554a5087a319))

## [0.1.7](https://github.com/oxy-hq/oxy-internal/compare/0.1.6...0.1.7) (2025-03-31)


### <!-- 0 -->🚀 Features

* add port support for serve ([#540](https://github.com/oxy-hq/oxy-internal/issues/540)) ([dbc7312](https://github.com/oxy-hq/oxy-internal/commit/dbc7312b30116d973c9832f569b0e696212be7c3))
* oxy make ([#538](https://github.com/oxy-hq/oxy-internal/issues/538)) ([00db9b7](https://github.com/oxy-hq/oxy-internal/commit/00db9b79c336e36535480f153b65a27e4c823502))

## [0.1.6](https://github.com/oxy-hq/oxy-internal/compare/0.1.5...0.1.6) (2025-03-28)


### <!-- 0 -->🚀 Features

* support running api and web on same port ([#536](https://github.com/oxy-hq/oxy-internal/issues/536)) ([a1e9ff5](https://github.com/oxy-hq/oxy-internal/commit/a1e9ff51f8b0c414ef67a7b4ed952fc0b8e3e307))

## [0.1.5](https://github.com/oxy-hq/oxy-internal/compare/0.1.4...0.1.5) (2025-03-28)


### <!-- 0 -->🚀 Features

* [#510](https://github.com/oxy-hq/oxy-internal/issues/510) delete thread ([#520](https://github.com/oxy-hq/oxy-internal/issues/520)) ([998b9dd](https://github.com/oxy-hq/oxy-internal/commit/998b9ddf2fd690311a7524352deb89a3d4ff1096))
* add clickhouse & mysql support ([#508](https://github.com/oxy-hq/oxy-internal/issues/508)) ([2415510](https://github.com/oxy-hq/oxy-internal/commit/2415510567b484b927299d7daf995ecfbe3f41f9))
* add self-update to check for latest update ([9cdb948](https://github.com/oxy-hq/oxy-internal/commit/9cdb948e54e2ccd5e4e54f1d45cc1e23a59041a4))
* delete all threads ([#526](https://github.com/oxy-hq/oxy-internal/issues/526)) ([74f8be6](https://github.com/oxy-hq/oxy-internal/commit/74f8be6d8907d445c48d3eb01960434738c40cc0))
* gemini support ([#521](https://github.com/oxy-hq/oxy-internal/issues/521)) ([a0855e2](https://github.com/oxy-hq/oxy-internal/commit/a0855e27986363ae28c702eb0ba642d34771aa72))
* self-update to check for latest update (closes [#132](https://github.com/oxy-hq/oxy-internal/issues/132)) ([#525](https://github.com/oxy-hq/oxy-internal/issues/525)) ([9cdb948](https://github.com/oxy-hq/oxy-internal/commit/9cdb948e54e2ccd5e4e54f1d45cc1e23a59041a4))
* support snowflake and read env from .env ([#527](https://github.com/oxy-hq/oxy-internal/issues/527)) ([be4d803](https://github.com/oxy-hq/oxy-internal/commit/be4d8039b2603b42f9bd20b87d6efa9c62b60eb0))


### <!-- 1 -->🐛 Bug Fixes

* **#410:** handling of multibytes char inside pluralizer ([#522](https://github.com/oxy-hq/oxy-internal/issues/522)) ([6b6e91d](https://github.com/oxy-hq/oxy-internal/commit/6b6e91dbd9af6bf965926382fdd6b172eeb814b5))
* figure out another port when one is busy ([#528](https://github.com/oxy-hq/oxy-internal/issues/528)) ([5477445](https://github.com/oxy-hq/oxy-internal/commit/54774451644b11e8d1e8d322ed5d7d73a3eceaec))
* get schema not working for snowflake ([#534](https://github.com/oxy-hq/oxy-internal/issues/534)) ([14f8a77](https://github.com/oxy-hq/oxy-internal/commit/14f8a77583c8d0250e45e825011b27f7c756cf38))

## [0.1.4](https://github.com/oxy-hq/oxy-internal/compare/0.1.3...0.1.4) (2025-03-21)

### <!-- 0 -->🚀 Features

- add sql query reference to agent answer ([#502](https://github.com/oxy-hq/oxy-internal/issues/502)) ([ce55c54](https://github.com/oxy-hq/oxy-internal/commit/ce55c541b1d60f88226c701898dec657847982d2))

## [0.1.3](https://github.com/oxy-hq/oxy-internal/compare/0.1.2...0.1.3) (2025-03-18)

### <!-- 1 -->🐛 Bug Fixes

- agent sql execution is not logged ([#494](https://github.com/oxy-hq/oxy-internal/issues/494)) ([5e703e9](https://github.com/oxy-hq/oxy-internal/commit/5e703e9ecbe3ad4babf78b39b914f28241ac8d61))

## [0.1.2](https://github.com/oxy-hq/oxy-internal/compare/0.1.1...0.1.2) (2025-03-18)

## [0.1.1](https://github.com/oxy-hq/oxy-internal/compare/0.1.0...0.1.1) (2025-03-01)

### <!-- 7 -->⚙️ Miscellaneous Tasks

- release 0.1.1 ([7f760a5](https://github.com/oxy-hq/oxy-internal/commit/7f760a5294f0896bc3295c7ec66a502131e3b5e5))

## [0.1.0](https://github.com/oxy-hq/oxy-internal/compare/v0.1.0...0.1.0) (2025-02-28)

### ⚠ BREAKING CHANGES

- change naming of some primites (steps, warehouses -> tasks, databases) ([#401](https://github.com/oxy-hq/oxy-internal/issues/401))

### <!-- 0 -->🚀 Features

- [#305](https://github.com/oxy-hq/oxy-internal/issues/305) Chat UI for Agent ([#346](https://github.com/oxy-hq/oxy-internal/issues/346)) ([0530f4c](https://github.com/oxy-hq/oxy-internal/commit/0530f4c9a5317f4d8c2fcc5f955799a91f676f4e))
- [#308](https://github.com/oxy-hq/oxy-internal/issues/308) Render Agent yaml config to desktop app UI ([#372](https://github.com/oxy-hq/oxy-internal/issues/372)) ([b171dbb](https://github.com/oxy-hq/oxy-internal/commit/b171dbbd5333efb9100f0c18f69e1f7d16b49e5a))
- [#382](https://github.com/oxy-hq/oxy-internal/issues/382) the execute_sql step within workflows works should apply Jinja context from the workflow ([#383](https://github.com/oxy-hq/oxy-internal/issues/383)) ([9248309](https://github.com/oxy-hq/oxy-internal/commit/9248309486f78bea6b5469236375f3f30dcd2e10))
- re-organize everything to prep for opensource ([#296](https://github.com/oxy-hq/oxy-internal/issues/296)) ([094bfb1](https://github.com/oxy-hq/oxy-internal/commit/094bfb1490f37dc828bfbd43887c2024eb7eae7d))
- subworkflow and workflow variables new ([#407](https://github.com/oxy-hq/oxy-internal/issues/407)) ([a3bfb5f](https://github.com/oxy-hq/oxy-internal/commit/a3bfb5f598ffc8bf8f30c93bcbee4e7e64d74f61))

### <!-- 1 -->🐛 Bug Fixes

- [#373](https://github.com/oxy-hq/oxy-internal/issues/373) chat with agent ([#374](https://github.com/oxy-hq/oxy-internal/issues/374)) ([b2bf835](https://github.com/oxy-hq/oxy-internal/commit/b2bf835a3fb2da4dae0ba1a6532bcde5400d0ed2))
- emojis with variant selector break tabled ([#370](https://github.com/oxy-hq/oxy-internal/issues/370)) ([86c4686](https://github.com/oxy-hq/oxy-internal/commit/86c46864f52aad7a209e93462838f5149a272300))
- handle oxy init when no config is found ([#340](https://github.com/oxy-hq/oxy-internal/issues/340)) ([5eae6e2](https://github.com/oxy-hq/oxy-internal/commit/5eae6e247059d055708c928b0347363c555a6e55))
- lock duckdb version ([#350](https://github.com/oxy-hq/oxy-internal/issues/350)) ([0fe2d10](https://github.com/oxy-hq/oxy-internal/commit/0fe2d10ada984f37e6cf96b0be8e0aa8af082013))
- refactor cache into executor ([#337](https://github.com/oxy-hq/oxy-internal/issues/337)) ([69e5557](https://github.com/oxy-hq/oxy-internal/commit/69e555744808917828c764ae918964a2ce660bac))
- update Windows DIST path to correct directory ([bc01c4b](https://github.com/oxy-hq/oxy-internal/commit/bc01c4bc4a29077382074ba6ae50c7cc2fbc721c))
- workaround for release please issue ([213d3a1](https://github.com/oxy-hq/oxy-internal/commit/213d3a175307b70eafeeba18e2e4718f3035d100))
- workaround for release please issue ([a685f57](https://github.com/oxy-hq/oxy-internal/commit/a685f57e25f8e8e198dd3fb035e4a161e796c5de))

### <!-- 7 -->⚙️ Miscellaneous Tasks

- release 0.1.0 ([3414da0](https://github.com/oxy-hq/oxy-internal/commit/3414da02943f3e6dd775c00a3de956263a2bb65a))
- release 0.1.38 ([49c44f2](https://github.com/oxy-hq/oxy-internal/commit/49c44f28d912de43c7042ff0768427d1243faff3))
- release 0.1.38 ([b10bc5c](https://github.com/oxy-hq/oxy-internal/commit/b10bc5c4d5d677cc2235d36135c8329e582da75a))

### <!-- 2 -->🚜 Refactor

- change naming of some primites (steps, warehouses -&gt; tasks, databases) ([#401](https://github.com/oxy-hq/oxy-internal/issues/401)) ([7705d6f](https://github.com/oxy-hq/oxy-internal/commit/7705d6fb8f30b0c2b2cc26f3910b95aefec0e80d))
