# Changelog

All notable changes to this project will be documented in this file.



## [0.2.5](https://github.com/onyx-hq/onyx-internal/compare/0.2.4...0.2.5) (2025-02-28)


### <!-- 7 -->⚙️ Miscellaneous Tasks

* release 0.2.4 ([87205b0](https://github.com/onyx-hq/onyx-internal/commit/87205b0f972bac196d01bebe762372e5b32f1c77))

## [0.2.4](https://github.com/onyx-hq/onyx-internal/compare/0.2.3...0.2.4) (2025-02-28)


### <!-- 7 -->⚙️ Miscellaneous Tasks

* release 0.2.4 ([1420779](https://github.com/onyx-hq/onyx-internal/commit/142077966cd20ad4038b4fe924b5de7024b57432))

## [0.2.3](https://github.com/onyx-hq/onyx-internal/compare/0.2.2...0.2.3) (2025-02-27)


### <!-- 7 -->⚙️ Miscellaneous Tasks

* release 0.2.3 ([819fbc6](https://github.com/onyx-hq/onyx-internal/commit/819fbc62255a00e0feefbbe01486a5561e4f7740))

## [0.2.2](https://github.com/onyx-hq/onyx-internal/compare/0.2.1...0.2.2) (2025-02-27)


### <!-- 7 -->⚙️ Miscellaneous Tasks

* release 0.2.2 ([2283d12](https://github.com/onyx-hq/onyx-internal/commit/2283d12daf74580619dde508d96b72a639ae654c))

## [0.2.1](https://github.com/onyx-hq/onyx-internal/compare/0.2.0...0.2.1) (2025-02-27)


### <!-- 7 -->⚙️ Miscellaneous Tasks

* release 0.2.1 ([a7e51d0](https://github.com/onyx-hq/onyx-internal/commit/a7e51d0d2abbab301e55d34db12668edc46c344b))

## [0.2.0](https://github.com/onyx-hq/onyx-internal/compare/0.1.38...0.2.0) (2025-02-27)


### ⚠ BREAKING CHANGES

* change naming of some primites (steps, warehouses -> tasks, databases) ([#401](https://github.com/onyx-hq/onyx-internal/issues/401))

### <!-- 0 -->🚀 Features

* [#308](https://github.com/onyx-hq/onyx-internal/issues/308) Render Agent yaml config to desktop app UI ([#372](https://github.com/onyx-hq/onyx-internal/issues/372)) ([b171dbb](https://github.com/onyx-hq/onyx-internal/commit/b171dbbd5333efb9100f0c18f69e1f7d16b49e5a))
* [#382](https://github.com/onyx-hq/onyx-internal/issues/382) the execute_sql step within workflows works should apply Jinja context from the workflow ([#383](https://github.com/onyx-hq/onyx-internal/issues/383)) ([9248309](https://github.com/onyx-hq/onyx-internal/commit/9248309486f78bea6b5469236375f3f30dcd2e10))
* subworkflow and workflow variables new ([#407](https://github.com/onyx-hq/onyx-internal/issues/407)) ([a3bfb5f](https://github.com/onyx-hq/onyx-internal/commit/a3bfb5f598ffc8bf8f30c93bcbee4e7e64d74f61))


### <!-- 2 -->🚜 Refactor

* change naming of some primites (steps, warehouses -&gt; tasks, databases) ([#401](https://github.com/onyx-hq/onyx-internal/issues/401)) ([7705d6f](https://github.com/onyx-hq/onyx-internal/commit/7705d6fb8f30b0c2b2cc26f3910b95aefec0e80d))

## [0.1.38](https://github.com/onyx-hq/onyx/compare/onyx-core-v0.1.38...onyx-core-0.1.38) (2025-02-20)


### <!-- 0 -->🚀 Features

* [#305](https://github.com/onyx-hq/onyx/issues/305) Chat UI for Agent ([#346](https://github.com/onyx-hq/onyx/issues/346)) ([0530f4c](https://github.com/onyx-hq/onyx/commit/0530f4c9a5317f4d8c2fcc5f955799a91f676f4e))
* re-organize everything to prep for opensource ([#296](https://github.com/onyx-hq/onyx/issues/296)) ([094bfb1](https://github.com/onyx-hq/onyx/commit/094bfb1490f37dc828bfbd43887c2024eb7eae7d))


### <!-- 1 -->🐛 Bug Fixes

* [#373](https://github.com/onyx-hq/onyx/issues/373) chat with agent ([#374](https://github.com/onyx-hq/onyx/issues/374)) ([b2bf835](https://github.com/onyx-hq/onyx/commit/b2bf835a3fb2da4dae0ba1a6532bcde5400d0ed2))
* emojis with variant selector break tabled ([#370](https://github.com/onyx-hq/onyx/issues/370)) ([86c4686](https://github.com/onyx-hq/onyx/commit/86c46864f52aad7a209e93462838f5149a272300))
* handle onyx init when no config is found ([#340](https://github.com/onyx-hq/onyx/issues/340)) ([5eae6e2](https://github.com/onyx-hq/onyx/commit/5eae6e247059d055708c928b0347363c555a6e55))
* lock duckdb version ([#350](https://github.com/onyx-hq/onyx/issues/350)) ([0fe2d10](https://github.com/onyx-hq/onyx/commit/0fe2d10ada984f37e6cf96b0be8e0aa8af082013))
* refactor cache into executor ([#337](https://github.com/onyx-hq/onyx/issues/337)) ([69e5557](https://github.com/onyx-hq/onyx/commit/69e555744808917828c764ae918964a2ce660bac))
* update Windows DIST path to correct directory ([bc01c4b](https://github.com/onyx-hq/onyx/commit/bc01c4bc4a29077382074ba6ae50c7cc2fbc721c))
* workaround for release please issue ([213d3a1](https://github.com/onyx-hq/onyx/commit/213d3a175307b70eafeeba18e2e4718f3035d100))
* workaround for release please issue ([a685f57](https://github.com/onyx-hq/onyx/commit/a685f57e25f8e8e198dd3fb035e4a161e796c5de))


### <!-- 7 -->⚙️ Miscellaneous Tasks

* release 0.1.38 ([49c44f2](https://github.com/onyx-hq/onyx/commit/49c44f28d912de43c7042ff0768427d1243faff3))
* release 0.1.38 ([b10bc5c](https://github.com/onyx-hq/onyx/commit/b10bc5c4d5d677cc2235d36135c8329e582da75a))

## [0.1.37](https://github.com/onyx-hq/onyx/compare/0.1.36...0.1.37) (2025-02-19)


### <!-- 1 -->🐛 Bug Fixes

* [#373](https://github.com/onyx-hq/onyx/issues/373) chat with agent ([#374](https://github.com/onyx-hq/onyx/issues/374)) ([b2bf835](https://github.com/onyx-hq/onyx/commit/b2bf835a3fb2da4dae0ba1a6532bcde5400d0ed2))

## [0.1.36](https://github.com/onyx-hq/onyx/compare/0.1.35...0.1.36) (2025-02-19)


### <!-- 1 -->🐛 Bug Fixes

* emojis with variant selector break tabled ([#370](https://github.com/onyx-hq/onyx/issues/370)) ([86c4686](https://github.com/onyx-hq/onyx/commit/86c46864f52aad7a209e93462838f5149a272300))


### <!-- 7 -->⚙️ Miscellaneous Tasks

* lock libduckdb sys ([38d5a70](https://github.com/onyx-hq/onyx/commit/38d5a703be3174dacc3591a4fb8d273272b41bd8))

## [0.1.35](https://github.com/onyx-hq/onyx/compare/0.1.34...0.1.35) (2025-02-17)


### <!-- 0 -->🚀 Features

* [#305](https://github.com/onyx-hq/onyx/issues/305) Chat UI for Agent ([#346](https://github.com/onyx-hq/onyx/issues/346)) ([0530f4c](https://github.com/onyx-hq/onyx/commit/0530f4c9a5317f4d8c2fcc5f955799a91f676f4e))


### <!-- 1 -->🐛 Bug Fixes

* lock duckdb version ([#350](https://github.com/onyx-hq/onyx/issues/350)) ([0fe2d10](https://github.com/onyx-hq/onyx/commit/0fe2d10ada984f37e6cf96b0be8e0aa8af082013))


### <!-- 2 -->🚜 Refactor

* unify arrow version and error handling inside connector ([#349](https://github.com/onyx-hq/onyx/issues/349)) ([17b0b03](https://github.com/onyx-hq/onyx/commit/17b0b037218770d8d2d699d7f4c85314c81d700a))

## [0.1.34](https://github.com/onyx-hq/onyx/compare/0.1.33...0.1.34) (2025-02-14)

### <!-- 1 -->🐛 Bug Fixes

- handle onyx init when no config is found ([#340](https://github.com/onyx-hq/onyx/issues/340)) ([5eae6e2](https://github.com/onyx-hq/onyx/commit/5eae6e247059d055708c928b0347363c555a6e55))
- refactor cache into executor ([#337](https://github.com/onyx-hq/onyx/issues/337)) ([69e5557](https://github.com/onyx-hq/onyx/commit/69e555744808917828c764ae918964a2ce660bac))

### <!-- 7 -->⚙️ Miscellaneous Tasks

- housekeeping the docs and run cargo clippy fix for all codes ([#342](https://github.com/onyx-hq/onyx/issues/342)) ([f3c59ea](https://github.com/onyx-hq/onyx/commit/f3c59ea9c88e7fbfca823aff3f9dbbc649f7b84d))
- remove default agents config, make defaults optional ([#336](https://github.com/onyx-hq/onyx/issues/336)) ([7bbc79e](https://github.com/onyx-hq/onyx/commit/7bbc79e0701c6a97cfb3963c626f8851367f9d63))
- remove unused dependencies ([67bf83a](https://github.com/onyx-hq/onyx/commit/67bf83a8b008c3cfc5a521be92e222a59231c7ac))

## [0.1.33](https://github.com/onyx-hq/onyx/compare/0.1.32...0.1.33) (2025-02-06)

### <!-- 0 -->🚀 Features

- re-organize everything to prep for opensource ([#296](https://github.com/onyx-hq/onyx/issues/296)) ([094bfb1](https://github.com/onyx-hq/onyx/commit/094bfb1490f37dc828bfbd43887c2024eb7eae7d))

### <!-- 1 -->🐛 Bug Fixes

- workaround for release please issue ([213d3a1](https://github.com/onyx-hq/onyx/commit/213d3a175307b70eafeeba18e2e4718f3035d100))
- workaround for release please issue ([a685f57](https://github.com/onyx-hq/onyx/commit/a685f57e25f8e8e198dd3fb035e4a161e796c5de))

## [0.1.32](https://github.com/onyx-hq/onyx/compare/0.1.31...0.1.32) (2025-02-06)

### <!-- 0 -->🚀 Features

- add enabled key under cache key ([#322](https://github.com/onyx-hq/onyx/issues/322)) ([9fa7f20](https://github.com/onyx-hq/onyx/commit/9fa7f2076010874b88a3106f7521cb1811a01fcb))
- add installation script for Onyx with support for Linux and macOS ([#284](https://github.com/onyx-hq/onyx/issues/284)) ([5f1a41c](https://github.com/onyx-hq/onyx/commit/5f1a41c0adf64aefb64df3048ef9f946565e5324))
- apply copybara to publish an opensource version of onyx ([#293](https://github.com/onyx-hq/onyx/issues/293)) ([14bfc69](https://github.com/onyx-hq/onyx/commit/14bfc699d34a121123aa5c2d96dbb9bbf7c4b415))
- create project selection and view project file tree ([#283](https://github.com/onyx-hq/onyx/issues/283)) ([8ee471b](https://github.com/onyx-hq/onyx/commit/8ee471b2a24f697c51e92332931f8ba02c6445f6))
- eng-1175 Create desktop app with Tauri ([#233](https://github.com/onyx-hq/onyx/issues/233)) ([a4a5d98](https://github.com/onyx-hq/onyx/commit/a4a5d9806beb312deeead7f548694a6166090427))
- enhance GitHub Actions workflow to retrieve GitHub App User ID and update checkout action ([6251638](https://github.com/onyx-hq/onyx/commit/6251638e2c6140ca4d86799e8923a16537ec88b2))
- include some build args to ensure onyxpy build succeed with cargo build ([dabe175](https://github.com/onyx-hq/onyx/commit/dabe175581e40defbad00ee5fdac5ad9c33aefc8))
- rename app to onyx-desktop and update dependencies in Cargo files ([3cf5cd3](https://github.com/onyx-hq/onyx/commit/3cf5cd30f80ce582045e2cd97d6dfb38c62198cf))
- support caching for generated queries inside workflows ([#290](https://github.com/onyx-hq/onyx/issues/290)) ([f66b869](https://github.com/onyx-hq/onyx/commit/f66b86993ea92eee8b8a683350b57599b7f76cde))
- update to new filesystem format, and use new positioning ([#323](https://github.com/onyx-hq/onyx/issues/323)) ([234a0ac](https://github.com/onyx-hq/onyx/commit/234a0ac802665f15b3189a187399b4abc502f2c5))

### <!-- 1 -->🐛 Bug Fixes

- correct committer format in GitHub Actions workflow for opensource publishing ([477ba94](https://github.com/onyx-hq/onyx/commit/477ba94bda42ddbd3ce115d2a8ba52c5ec94f2e6))
- correct syntax for moving tauri assets in release workflow ([939f37f](https://github.com/onyx-hq/onyx/commit/939f37fe9b523105ee178058c7ef037163d84036))
- enable caching for all Rust crates in CI workflows ([7043f76](https://github.com/onyx-hq/onyx/commit/7043f76a0e9f7b3704d2ee397599ff97f3b6f06b))
- prefix artifact names with 'tauri-' and 'cli-' in release workflow ([caa5332](https://github.com/onyx-hq/onyx/commit/caa5332a5468b9aef27a7e6a47889fef1279b405))
- update lint-staged command to handle scheduled events differently ([2887a3b](https://github.com/onyx-hq/onyx/commit/2887a3b70ebb6a872901185684add2fd0ec67cfe))
- update SSH key reference in GitHub Actions workflow for Copybara ([69e6280](https://github.com/onyx-hq/onyx/commit/69e628034dc46e56f547a593d5f997b7b92211ce))
- update tauri asset movement in release workflow for correct handling ([c75a3a7](https://github.com/onyx-hq/onyx/commit/c75a3a70e3858a5c75f3a1027c5cb0b056dd68a7))

### <!-- 3 -->📚 Documentation

- update agents and semantic model documentation with configuration and usage examples ([#303](https://github.com/onyx-hq/onyx/issues/303)) ([ace309f](https://github.com/onyx-hq/onyx/commit/ace309f42edf40a8cf26e1970158c06ca801c9bb))

### <!-- 7 -->⚙️ Miscellaneous Tasks

- increase lts version for node ([e56d458](https://github.com/onyx-hq/onyx/commit/e56d45834c900c9ae95ce0e16a856181669eb28f))
- remove commit-msg hook dependency on app web ([cd29134](https://github.com/onyx-hq/onyx/commit/cd29134727c1c281f9e8b9b96952126c0a295b85))
- rename install-desktop.sh to install_desktop.sh and add cleanup logic ([18ffe13](https://github.com/onyx-hq/onyx/commit/18ffe13b4fe812661f26f6ecf15ab2800c4a44c9))
- update example ([bd9f4ff](https://github.com/onyx-hq/onyx/commit/bd9f4ff2768743b6a8d7d33cfca58c534e092a4c))
- update example ([4b37490](https://github.com/onyx-hq/onyx/commit/4b3749050cb8023e491b24d916e0efe476e2d899))
- update examples ([be9ab6c](https://github.com/onyx-hq/onyx/commit/be9ab6c5fb051c856e0736dd824e4f6079e0f2d0))

### <!-- 2 -->🚜 Refactor

- remove create-cache workflow and update release workflow for tauri and CLI builds ([b7fd7de](https://github.com/onyx-hq/onyx/commit/b7fd7dede3ab353db52516058e73f7bbcd49d076))

### <!-- 10 -->💼 Build System

- **deps-dev:** bump eslint-config-prettier from 9.1.0 to 10.0.1 in the dev-npm-major-dependencies group ([#281](https://github.com/onyx-hq/onyx/issues/281)) ([f5b77cc](https://github.com/onyx-hq/onyx/commit/f5b77cc62da376ccf755a9f24a859f95e0744bb4))
- **deps-dev:** bump vite from 5.4.10 to 5.4.12 in /web-app in the npm_and_yarn group across 1 directory ([#286](https://github.com/onyx-hq/onyx/issues/286)) ([cb8f810](https://github.com/onyx-hq/onyx/commit/cb8f810129d6676565cee95126c5a1fb84b1a3ca))
- **deps-dev:** bump vite from 5.4.10 to 5.4.12 in /web-app in the npm_and_yarn group across 1 directory ([#292](https://github.com/onyx-hq/onyx/issues/292)) ([00d6fc8](https://github.com/onyx-hq/onyx/commit/00d6fc8033178a195483586233a3525ce55be5cd))
- **deps-dev:** bump vite from 5.4.14 to 6.0.11 in the dev-npm-major-dependencies group ([#295](https://github.com/onyx-hq/onyx/issues/295)) ([a58a680](https://github.com/onyx-hq/onyx/commit/a58a680e6745302d1439ee7797d79259301063b4))
- **deps:** bump openssl from 0.10.68 to 0.10.70 in the cargo group across 1 directory ([#316](https://github.com/onyx-hq/onyx/issues/316)) ([fea241c](https://github.com/onyx-hq/onyx/commit/fea241c12e9f56a63337681d38cffb4fb14373f1))
- **deps:** bump react-router-dom from 6.28.0 to 7.1.3 in the prod-npm-major-dependencies group ([#280](https://github.com/onyx-hq/onyx/issues/280)) ([4181393](https://github.com/onyx-hq/onyx/commit/418139327dabc73b61c4b1da1581c4b243a07155))
- **deps:** bump react-router-dom from 6.28.0 to 7.1.5 in the prod-npm-major-dependencies group across 1 directory ([#317](https://github.com/onyx-hq/onyx/issues/317)) ([35ee016](https://github.com/onyx-hq/onyx/commit/35ee0164dfca851454f61e4c632be88fb670e66f))
- **deps:** bump the npm_and_yarn group with 2 updates ([#285](https://github.com/onyx-hq/onyx/issues/285)) ([d1c2632](https://github.com/onyx-hq/onyx/commit/d1c263213a338898251d80632818c12898635a8d))
- **deps:** bump the prod-npm-minor-dependencies group with 13 updates ([#278](https://github.com/onyx-hq/onyx/issues/278)) ([ff84a42](https://github.com/onyx-hq/onyx/commit/ff84a428e150d0d107cf700280df08a4281465ff))
- **deps:** update pnpm to version 9.15.4 in package.json files ([d74164e](https://github.com/onyx-hq/onyx/commit/d74164e4d7a2123abdee37873ecb4252f3699ef5))

### <!-- 11 -->💼 Continuous Integration

- enhance release workflow with unique identifier and improved tag description ([b5e4049](https://github.com/onyx-hq/onyx/commit/b5e404986cf0e486065868828bbc5ea400576bb2))
- fix output variable naming in public release workflow ([4af4798](https://github.com/onyx-hq/onyx/commit/4af47986eff820c1e7cc44f305221687b9203ab2))
- fix syntax error in public release workflow for tagging ([94de907](https://github.com/onyx-hq/onyx/commit/94de9078fe4cd84ed5ea489c696ecc5d5ca61d9c))
- update release workflow and fix formatting in package.json ([5bbcccd](https://github.com/onyx-hq/onyx/commit/5bbcccd9c0c76cb624dc7210e1ae40620868f000))
- update release workflow to create RELEASE_NOTES file before appending release notes ([8857f3c](https://github.com/onyx-hq/onyx/commit/8857f3ca0fa176e420b7643c91b98109228189e5))

## [0.1.31](https://github.com/onyx-hq/onyx/compare/0.1.30...0.1.31) (2025-01-17)

### <!-- 0 -->🚀 Features

- add api url for openai model to support azure openai ([#184](https://github.com/onyx-hq/onyx/issues/184)) ([a1ae734](https://github.com/onyx-hq/onyx/commit/a1ae734a6f47b9689adce826aaf85c727c1adba9))
- add integration tests ([#219](https://github.com/onyx-hq/onyx/issues/219)) ([e35258f](https://github.com/onyx-hq/onyx/commit/e35258f3d32ad4a4bf8368854690f903a8d61c74))
- **eng-1173:** Support export argument on workflow steps ([#198](https://github.com/onyx-hq/onyx/issues/198)) ([98b73ab](https://github.com/onyx-hq/onyx/commit/98b73ab6ba63db5b99c12f9a5d54286671daf679))
- hybrid search ([#192](https://github.com/onyx-hq/onyx/issues/192)) ([019c2e9](https://github.com/onyx-hq/onyx/commit/019c2e95d30db5e737e3672028634a0c5932caec))
- implement execution progress ([#229](https://github.com/onyx-hq/onyx/issues/229)) ([671d60d](https://github.com/onyx-hq/onyx/commit/671d60d6de14d0cd4f579f6654a2d64331b38dd6))
- improve logging and error handling ([#209](https://github.com/onyx-hq/onyx/issues/209)) ([bf71df2](https://github.com/onyx-hq/onyx/commit/bf71df23714efe10a59b520e72d03e4f5547e412))
- refactor context ([#211](https://github.com/onyx-hq/onyx/issues/211)) ([aa30d4a](https://github.com/onyx-hq/onyx/commit/aa30d4aaf0e8779ad4893a74585f16b02d8e31c8))

### <!-- 1 -->🐛 Bug Fixes

- add JSON schema validation step to CI workflows ([97d13f1](https://github.com/onyx-hq/onyx/commit/97d13f13be63114f64a481f63f9ae60871893b7d))
- export panic when execute_sql failed ([#246](https://github.com/onyx-hq/onyx/issues/246)) ([1ce83aa](https://github.com/onyx-hq/onyx/commit/1ce83aadf1002436126206e5019c6227f4b0d403))
- key_path should not be a required argument when warehouse.type = duckdb ([#193](https://github.com/onyx-hq/onyx/issues/193)) ([b645c9a](https://github.com/onyx-hq/onyx/commit/b645c9acc568071dabfaa1b36b995c04f4c69d7f))
- update to onyx run syntax ([#212](https://github.com/onyx-hq/onyx/issues/212)) ([b46250c](https://github.com/onyx-hq/onyx/commit/b46250cc838aee6f912cf25d75b6bd4f3217f495))
- validate result not showed on error ([#213](https://github.com/onyx-hq/onyx/issues/213)) ([ed12f46](https://github.com/onyx-hq/onyx/commit/ed12f4666254b1b12529e5c8506deb4b374a6adb))

### <!-- 3 -->📚 Documentation

- add pull request template ([9008ba6](https://github.com/onyx-hq/onyx/commit/9008ba6b9adcf966fbb35a85f32d26cb00b25379))
- add pull request template ([ae824b6](https://github.com/onyx-hq/onyx/commit/ae824b6a15a7e4d1868241c86ad4bdee09b7ed3f))
- add release guideline ([#218](https://github.com/onyx-hq/onyx/issues/218)) ([51c1883](https://github.com/onyx-hq/onyx/commit/51c1883f1d2d5337411676bd10320467c78ae33f))
- refine pull request template by removing unnecessary header ([0979fc2](https://github.com/onyx-hq/onyx/commit/0979fc2f80576de4c8815ab262947d45866fc7b7))
- update pull request template to enhance test plan and checklist sections ([68821ab](https://github.com/onyx-hq/onyx/commit/68821ab338d8d3807154af606c5e6c1a57b102b9))

### <!-- 7 -->⚙️ Miscellaneous Tasks

- remove unused dependencies ([#274](https://github.com/onyx-hq/onyx/issues/274)) ([a39c154](https://github.com/onyx-hq/onyx/commit/a39c1542ba801375bda8a3d7aaa76e594245326e))
- remove unused deps ([#197](https://github.com/onyx-hq/onyx/issues/197)) ([7a8c9da](https://github.com/onyx-hq/onyx/commit/7a8c9daba943134fbe5fe99d58b1789411d249e7))
- update dependabot schedule ([0b0e685](https://github.com/onyx-hq/onyx/commit/0b0e685177fab573e6c846ec66df20a324023796))
- update json schemas ([7c1dc57](https://github.com/onyx-hq/onyx/commit/7c1dc574fe46217b8a4bedf4103f34ba023ae9bc))

### <!-- 10 -->💼 Build System

- **deps-dev:** bump @types/node from 22.10.2 to 22.10.5 ([#228](https://github.com/onyx-hq/onyx/issues/228)) ([b908026](https://github.com/onyx-hq/onyx/commit/b908026f266373c0cfc2a105f78e3a21654ff9a5))
- **deps-dev:** bump @vitejs/plugin-react-swc from 3.7.1 to 3.7.2 ([#201](https://github.com/onyx-hq/onyx/issues/201)) ([54e7aad](https://github.com/onyx-hq/onyx/commit/54e7aadf5798d6c67be53ccba014528e9af0ae08))
- **deps-dev:** bump eslint from 9.14.0 to 9.17.0 ([#225](https://github.com/onyx-hq/onyx/issues/225)) ([9aa0876](https://github.com/onyx-hq/onyx/commit/9aa0876c94a2a784a8c90f485415a6af4859f0e5))
- **deps-dev:** bump eslint-plugin-react-hooks from 5.1.0-rc-fb9a90fa48-20240614 to 5.1.0 ([#200](https://github.com/onyx-hq/onyx/issues/200)) ([85d7609](https://github.com/onyx-hq/onyx/commit/85d76094d0a79ef832f81cdb2134fc1f2ed0bc62))
- **deps-dev:** bump eslint-plugin-unicorn from 56.0.0 to 56.0.1 ([#224](https://github.com/onyx-hq/onyx/issues/224)) ([1ea6242](https://github.com/onyx-hq/onyx/commit/1ea62425fe292db027fa898ce3e1ebcb19a0afd7))
- **deps-dev:** bump husky from 9.1.6 to 9.1.7 ([#227](https://github.com/onyx-hq/onyx/issues/227)) ([a67ac57](https://github.com/onyx-hq/onyx/commit/a67ac579fc44c199eeacf38735e82aff2280e765))
- **deps-dev:** bump prettier from 3.4.1 to 3.4.2 ([#165](https://github.com/onyx-hq/onyx/issues/165)) ([142ef3a](https://github.com/onyx-hq/onyx/commit/142ef3a79150364a4b1741491c80642ec14af5bf))
- **deps-dev:** bump the dev-npm-minor-dependencies group across 1 directory with 20 updates ([#234](https://github.com/onyx-hq/onyx/issues/234)) ([c757b0c](https://github.com/onyx-hq/onyx/commit/c757b0ca42ff02c75c9bfdf6759b88e2fa77ed14))
- **deps-dev:** bump typescript-eslint from 8.14.0 to 8.19.0 ([#214](https://github.com/onyx-hq/onyx/issues/214)) ([c14f972](https://github.com/onyx-hq/onyx/commit/c14f972b13c33a3c2582ca36ac58acf4cbc16b8d))
- **deps:** bump @radix-ui/react-switch from 1.1.1 to 1.1.2 ([#226](https://github.com/onyx-hq/onyx/issues/226)) ([7d16b82](https://github.com/onyx-hq/onyx/commit/7d16b82dd6369e2cb2413911b950fef4a91037ff))
- **deps:** bump @uiw/codemirror-themes from 4.23.6 to 4.23.7 ([#199](https://github.com/onyx-hq/onyx/issues/199)) ([0104c08](https://github.com/onyx-hq/onyx/commit/0104c08981d28b4870bece5895fcf0cb44e0e9b4))
- **deps:** bump async-trait from 0.1.83 to 0.1.85 ([#223](https://github.com/onyx-hq/onyx/issues/223)) ([162a53d](https://github.com/onyx-hq/onyx/commit/162a53d94e700806541aa072071820292eb88ccd))
- **deps:** bump axum-streams from 0.19.0 to 0.20.0 ([#221](https://github.com/onyx-hq/onyx/issues/221)) ([1e4ef81](https://github.com/onyx-hq/onyx/commit/1e4ef814f864f502249f04946328f7a75fcb89f8))
- **deps:** bump garde from 0.20.0 to 0.21.0 ([#217](https://github.com/onyx-hq/onyx/issues/217)) ([66e6f04](https://github.com/onyx-hq/onyx/commit/66e6f047c295b9810827fc87ef0ed99655fc2418))
- **deps:** bump glob from 0.3.1 to 0.3.2 ([#220](https://github.com/onyx-hq/onyx/issues/220)) ([590a7dc](https://github.com/onyx-hq/onyx/commit/590a7dca94cd5b3d6b034f233855a0e3d6a28cef))
- **deps:** bump openai from 4.72.0 to 4.77.0 ([#203](https://github.com/onyx-hq/onyx/issues/203)) ([b866a8f](https://github.com/onyx-hq/onyx/commit/b866a8fef50065095505b918b88c62eb8668b2ab))
- **deps:** bump reqwest from 0.12.11 to 0.12.12 ([#222](https://github.com/onyx-hq/onyx/issues/222)) ([5ff60a3](https://github.com/onyx-hq/onyx/commit/5ff60a312052392c7da7ba80cd67fc5963397200))
- **deps:** bump reqwest from 0.12.9 to 0.12.11 ([#215](https://github.com/onyx-hq/onyx/issues/215)) ([b1e06ea](https://github.com/onyx-hq/onyx/commit/b1e06eac109946ae1bd647dcf4e6ca4dd38bebf1))
- **deps:** bump serde from 1.0.216 to 1.0.217 ([#216](https://github.com/onyx-hq/onyx/issues/216)) ([13a59b8](https://github.com/onyx-hq/onyx/commit/13a59b87bb1bfc5115b5aedf4f0f692bc6c1cb4b))
- **deps:** bump thiserror from 1.0.69 to 2.0.7 ([#170](https://github.com/onyx-hq/onyx/issues/170)) ([489b8ac](https://github.com/onyx-hq/onyx/commit/489b8ac4bbd6e5f860b896c5fb17941eaf4f3acb))

### <!-- 11 -->💼 Continuous Integration

- add broken link check and typo check ([#276](https://github.com/onyx-hq/onyx/issues/276)) ([e3acf96](https://github.com/onyx-hq/onyx/commit/e3acf962ddb3c51a209a24b9f08c27b24a600e44))
- change slack action to use env ([d807b43](https://github.com/onyx-hq/onyx/commit/d807b435892915a7f8926cf29e1e224be7d49d35))
- move generation of config schema to another job ([e2a51a9](https://github.com/onyx-hq/onyx/commit/e2a51a95d28874ddccec873b0ca77da667211b73))

## [0.1.30](https://github.com/onyx-hq/onyx/compare/0.1.29...0.1.30) (2024-12-18)

### <!-- 1 -->🐛 Bug Fixes

- code review ([ffd79d2](https://github.com/onyx-hq/onyx/commit/ffd79d224651205c4b02fa158d4d3169bc5270d5))

### <!-- 11 -->💼 Continuous Integration

- add GH_TOKEN environment variable for release tag declaration ([50d1bf1](https://github.com/onyx-hq/onyx/commit/50d1bf13e83cd0e5bd38f9730bca838b1bac1bb5))
- allow artifacts to be merged ([319df1e](https://github.com/onyx-hq/onyx/commit/319df1eaead508e29de91e2b83c70b5904f837b7))
- ensure releases are executable ([e50ce4a](https://github.com/onyx-hq/onyx/commit/e50ce4ac839344be20e755d5f3de56341dbc4d69))
- fix output variable name for release tag in public release workflow ([e6b945b](https://github.com/onyx-hq/onyx/commit/e6b945bbb2ac938fa1eb0b538ef4a66f4c06c17e))
- make tag input required for release workflow ([8211d44](https://github.com/onyx-hq/onyx/commit/8211d449f4ac63b96f42ffee87f3ea52a6c57324))
- prioritize tag input over ref name in release workflow ([847642b](https://github.com/onyx-hq/onyx/commit/847642bf3a7cef8582c27a17e02656378a03ef2e))
- support passing tag to checking out ([03a3366](https://github.com/onyx-hq/onyx/commit/03a336686ae886fd3dab17563a1df1ff578c10b8))
- support passing tag to manual release ([4b7899e](https://github.com/onyx-hq/onyx/commit/4b7899e1b3bfba1fa21a0cabfd462ee9dc198c14))
- update job name to clarify binary compilation in release workflow ([a974678](https://github.com/onyx-hq/onyx/commit/a974678c9ed179c2cff4500125d23c2aef74cff4))

## [0.1.29](https://github.com/onyx-hq/onyx/compare/0.1.28...0.1.29) (2024-12-17)

### <!-- 11 -->💼 Continuous Integration

- add missing artifacts dir ([d2d929d](https://github.com/onyx-hq/onyx/commit/d2d929dc264560265f07a4e885757c62bb43dcc5))
- remove draft setting or else tags wont be published ([157e6e1](https://github.com/onyx-hq/onyx/commit/157e6e1f0892e885736c690b4a8f40bae901f763))

## [0.1.28](https://github.com/onyx-hq/onyx/compare/0.1.27...0.1.28) (2024-12-17)

### <!-- 7 -->⚙️ Miscellaneous Tasks

- comment out update json schemas to save bandwidth ([2eb4cca](https://github.com/onyx-hq/onyx/commit/2eb4ccac65a0d14c256dda3511a205023f292732))
- **main:** release 0.1.28 ([#180](https://github.com/onyx-hq/onyx/issues/180)) ([a4c2ae7](https://github.com/onyx-hq/onyx/commit/a4c2ae7dca08c816c86b5e05629c2abbd23d1a68))
- **main:** release 0.1.28 ([#185](https://github.com/onyx-hq/onyx/issues/185)) ([c194488](https://github.com/onyx-hq/onyx/commit/c19448875cc8c1812f9f11aacdf2f431e97a8835))
- **main:** release 0.1.29 ([#186](https://github.com/onyx-hq/onyx/issues/186)) ([a16530a](https://github.com/onyx-hq/onyx/commit/a16530a8180c11fa404ace93fba59e8a28a101dd))
- **main:** release 0.1.30 ([3eaf806](https://github.com/onyx-hq/onyx/commit/3eaf8060d86f9f64b4e257332caf0eedb2e1a526))

### <!-- 11 -->💼 Continuous Integration

- change release please settings ([6af45ba](https://github.com/onyx-hq/onyx/commit/6af45baccfa9346b516f83a92dd7833da316a931))
- release please bootstrap sha ([09d91a2](https://github.com/onyx-hq/onyx/commit/09d91a2f4b0cd70576ee035bccd34723027a8a41))

## [0.1.27](https://github.com/onyx-hq/onyx/compare/0.1.26...0.1.27) (2024-12-17)

### <!-- 0 -->🚀 Features

- add release please bootstrap ([38data29](https://github.com/onyx-hq/onyx/commit/38daa29b536cec13dae5629ebd6da6c38b92397d))
- **ENG-1167:** separate out queries from tool context rename as context ([#171](https://github.com/onyx-hq/onyx/issues/171)) ([250c7b9](https://github.com/onyx-hq/onyx/commit/250c7b9a6a0c60ac404401027af5091245c6ec8a))
- **ENG-1171:** allow for default warehouse argument in configyml ([#174](https://github.com/onyx-hq/onyx/issues/174)) ([2b6b803](https://github.com/onyx-hq/onyx/commit/2b6b803834d923172ab630785a5062dd4d32e9e2))
- remove action rust lang because it makes caching harder ([#160](https://github.com/onyx-hq/onyx/issues/160)) ([f11c73e](https://github.com/onyx-hq/onyx/commit/f11c73e5f8133b161f77f801ee07c52515b3cfa7))
- retry [#158](https://github.com/onyx-hq/onyx/issues/158) automatic release by combining release-plz and release-please ([#161](https://github.com/onyx-hq/onyx/issues/161)) ([cf333b2](https://github.com/onyx-hq/onyx/commit/cf333b2f4e1d0ccbdcd80a23f80188c596b851c7))
- support taking in onyx version for installation script ([a94b7cf](https://github.com/onyx-hq/onyx/commit/a94b7cf1bc595ec6bfdf36b8de66cd517561b037))

### <!-- 1 -->🐛 Bug Fixes

- installation script missing arm64 ([07ba05f](https://github.com/onyx-hq/onyx/commit/07ba05fdfdd0d1971cdae9c49c9437a96f700619))

### <!-- 7 -->⚙️ Miscellaneous Tasks

- add release-type rust ([39563e6](https://github.com/onyx-hq/onyx/commit/39563e6087db3c790d565a0bf47dc151404472f0))
- add some configs for release-please ([35b88b6](https://github.com/onyx-hq/onyx/commit/35b88b6f6aad5ef89d40cc94aac435e691010cf3))
- bootstrap releases for path: . ([a497ecd](https://github.com/onyx-hq/onyx/commit/a497ecda9bddc64d148770cac900af65332aa4bd))
- ignore label autorelease when running ci ([f281e0a](https://github.com/onyx-hq/onyx/commit/f281e0a38932746ef023f34e2b130471fb4fc14f))
- **main:** release 0.1.27 ([#178](https://github.com/onyx-hq/onyx/issues/178)) ([c7347de](https://github.com/onyx-hq/onyx/commit/c7347de36c3da942718cc09791946bd4e5c37931))
- **main:** release 0.2.0 ([#173](https://github.com/onyx-hq/onyx/issues/173)) ([a5f60d6](https://github.com/onyx-hq/onyx/commit/a5f60d6ab265fa51351b7c90bc3bfe8ab0f0ed68))
- match release manifest with current ver ([b5a3db6](https://github.com/onyx-hq/onyx/commit/b5a3db6e54ae9ad49db4e046435e215b375c2184))
- release please should use draft ([f39231e](https://github.com/onyx-hq/onyx/commit/f39231e38ee713923cb4ef8d89fbfbe514b431b5))
- remove excessive steps ([dfdd2ad](https://github.com/onyx-hq/onyx/commit/dfdd2adacb73495979815a50e2eac2ce329d2137))

### <!-- 10 -->💼 Build System

- **deps-dev:** bump @types/node from 20.17.6 to 22.10.2 ([#164](https://github.com/onyx-hq/onyx/issues/164)) ([6e5ad0f](https://github.com/onyx-hq/onyx/commit/6e5ad0f41e8f2dcbb8860a51fb3eabc012e020cd))
- **deps-dev:** bump eslint-plugin-react-refresh from 0.4.14 to 0.4.16 ([#163](https://github.com/onyx-hq/onyx/issues/163)) ([a86b1cc](https://github.com/onyx-hq/onyx/commit/a86b1cc3858e60be29a484fa1be4cf7109cba985))
- **deps-dev:** bump lint-staged from 15.2.10 to 15.2.11 ([#162](https://github.com/onyx-hq/onyx/issues/162)) ([1c44a0d](https://github.com/onyx-hq/onyx/commit/1c44a0d4aae7205376bca9b73e662c21881c273a))
- **deps:** bump ahooks from 3.8.1 to 3.8.4 ([#166](https://github.com/onyx-hq/onyx/issues/166)) ([693f0ca](https://github.com/onyx-hq/onyx/commit/693f0cafc9891a65b8f89f7fcf3b9e93ab11d841))
- **deps:** bump async-openai from 0.24.1 to 0.26.0 ([#167](https://github.com/onyx-hq/onyx/issues/167)) ([27806f6](https://github.com/onyx-hq/onyx/commit/27806f6d8b0ecd45818088a163c2cb5c1cea8ffb))
- **deps:** bump home from 0.5.9 to 0.5.11 ([#168](https://github.com/onyx-hq/onyx/issues/168)) ([adf2ddc](https://github.com/onyx-hq/onyx/commit/adf2ddc49799096500eea46dc44916d2267d0aff))

### <!-- 11 -->💼 Continuous Integration

- add tag true ([4e97461](https://github.com/onyx-hq/onyx/commit/4e974619fd51de26ee5dbabd1b42f653f188535b))
- adjust release-please ([f6eccfc](https://github.com/onyx-hq/onyx/commit/f6eccfc8e47b8e127e5110ba8311a49fd0495c1f))
- change config for release please action ([0d09dc9](https://github.com/onyx-hq/onyx/commit/0d09dc933a92903969983da40096336f28c4fe1d))
- change order of runs and unify json schemas into prep release ([f8bb50c](https://github.com/onyx-hq/onyx/commit/f8bb50c2f3a19f0c018f2e90787664ba34432eb4))
- enable github release for release-please ([99e50c7](https://github.com/onyx-hq/onyx/commit/99e50c74d8500ae2f46a18630329106f7143ac9b))
- fix path for release artifacts ([42cc93c](https://github.com/onyx-hq/onyx/commit/42cc93c106fb78567c9572f613ff0d3434f71053))
- ignore ci when running on release branch ([176d43f](https://github.com/onyx-hq/onyx/commit/176d43fd84650ea73fbf068dcd3cd10ab16ee0b8))
- try to sync configuration of release please with git cliff ([3046031](https://github.com/onyx-hq/onyx/commit/304603102cb3d2037ae6305c6e3567fe98ba65a8))
- unify into release-please ([cbdbb8c](https://github.com/onyx-hq/onyx/commit/cbdbb8c6ccb980907a222b3837fe04a5c8907337))
- update condition for CI to run when autoreleasing ([629846b](https://github.com/onyx-hq/onyx/commit/629846b4d8cea0489d44088f1877160415542839))

## [onyx-v0.1.24] - 2024-12-16

### 🚀 Features

- Prep for semantic release automatically (#158)

### 📚 Documentation

- Update welcome documentation titles and links
- Add workstation setup guide and beginner resources
- Restructure beginner resources and add new guides

## [0.1.25] - 2024-12-13

### 🐛 Bug Fixes

- Remove extra idx from keyword replacement (#157)

## [0.1.24] - 2024-12-13

### 🚀 Features

- Support mapping anonymization (#153)

### 🐛 Bug Fixes

- Serve with agent relative path (#150)

### 💼 Other

- _(deps-dev)_ Bump vite from 5.4.11 to 6.0.3
- _(deps-dev)_ Bump vite from 5.4.11 to 6.0.3

### 📚 Documentation

- Add CLI shortcut and command references (#149)

### ⚙️ Miscellaneous Tasks

- Move everything to examples
- Bump version

## [0.1.23] - 2024-12-11

### 🐛 Bug Fixes

- Build error after upgrading react and types/react

### 💼 Other

- _(deps)_ Bump react-dom and @types/react-dom (#138)

### ⚙️ Miscellaneous Tasks

- Bump version

## [0.1.22] - 2024-12-11

### 🐛 Bug Fixes

- Refactor code problems with cargo

### 💼 Other

- _(deps)_ Bump pyo3 from 0.23.2 to 0.23.3 in the cargo group (#146)
- _(deps)_ Bump chrono from 0.4.38 to 0.4.39 (#140)
- _(deps-dev)_ Bump globals from 15.12.0 to 15.13.0 (#137)
- _(deps-dev)_ Bump eslint-plugin-promise from 7.1.0 to 7.2.1 (#135)
- _(deps)_ Bump react and @types/react (#136)

### ⚙️ Miscellaneous Tasks

- Bump version

## [0.1.21] - 2024-12-10

### 🚀 Features

- Narrow down mac os x deployment MACOSX_DEPLOYMENT_TARGET

### 🐛 Bug Fixes

- Add extension module so cargo build succeeds
- Merge main

### 🚜 Refactor

- Remove deprecated python setups

### ⚙️ Miscellaneous Tasks

- Bump version

## [0.1.20] - 2024-12-10

### 🚀 Features

- Clean up python projects

### 🐛 Bug Fixes

- Merge main
- Merge main

### ⚙️ Miscellaneous Tasks

- Bump version

## [0.1.19] - 2024-12-10

### ⚙️ Miscellaneous Tasks

- Bump version

## [0.1.18] - 2024-12-09

### 🐛 Bug Fixes

- Merge main
- Clean code
- Typo, unuse code
- Only show footer when total_column > displayed_column
- Update all config file
- Remove unuse code
- Bug load agent name
- Fmt
- Workflow for release

### ⚙️ Miscellaneous Tasks

- Fallback to default fetching behaviour for add and commit
- Bump version

## [0.1.17] - 2024-12-05

### 🚀 Features

- Add json schema to loop

### 🐛 Bug Fixes

- Public release file patterns

### 💼 Other

- Add gen config schema to release step

### ⚙️ Miscellaneous Tasks

- Regenerate json-schemas
- Disable windows
- Render jsonl
- Change path from schemas to json schemas
- Remove gen config schema from ci
- Update workflows
- Bump version

## [0.1.16] - 2024-12-04

### 🐛 Bug Fixes

- Set-output command is deprecated and will be disabled soon
- Add format and add remote url to config.json

### ⚙️ Miscellaneous Tasks

- Upload json schema together with onyx bin
- Add a step to override old schemas
- Bump version

## [0.1.15] - 2024-12-04

### 🚀 Features

- Make it so tables are responsive in terminal #95
- Add table output type

### 🐛 Bug Fixes

- Merge main
- Print the result table even batches null

### 💼 Other

- _(deps)_ Bump tokio from 1.41.0 to 1.41.1 (#120)
- _(deps-dev)_ Bump prettier from 3.3.3 to 3.4.1 (#127)
- _(deps)_ Bump axum from 0.7.7 to 0.7.9 (#122)
- _(deps-dev)_ Bump eslint-plugin-sonarjs from 2.0.4 to 3.0.0 (#123)
- _(deps)_ Bump minijinja from 2.4.0 to 2.5.0 (#121)
- _(deps)_ Bump backon from 1.2.0 to 1.3.0 (#119)
- _(deps)_ Bump rsa from 0.9.6 to 0.9.7 in the cargo group (#115)
- _(deps-dev)_ Bump @commitlint/cli from 19.5.0 to 19.6.0 (#125)
- _(deps)_ Bump match-sorter from 7.0.0 to 8.0.0 (#124)

### 🚜 Refactor

- Shorten the binary name

### 📚 Documentation

- Update installation command

### ⚙️ Miscellaneous Tasks

- Install script for windows
- Allow onyx fmt to run on main branch again
- Dont run on main branch
- Bump version

## [0.1.14] - 2024-12-02

### 🚀 Features

- Anonymize data
- Implement pluralize and case_insensitive
- Print deanonymized output

### 💼 Other

- Enable windows build (#117)

### ⚙️ Miscellaneous Tasks

- Add clear cache workflow and ignore docs folder
- File relative comment
- Fmt
- Bump version

## [0.1.13] - 2024-11-29

### 🐛 Bug Fixes

- Remove example config

### ⚙️ Miscellaneous Tasks

- Remove reference to example config

## [0.1.12] - 2024-11-29

### ⚙️ Miscellaneous Tasks

- Change all to ref_name
- Bump version

## [NightlyBuild_2024.11.29.run_109] - 2024-11-28

### ⚙️ Miscellaneous Tasks

- Unify release into one job
- Onyx run should just run the query (#110)

## [0.1.11] - 2024-11-28

### 🐛 Bug Fixes

- Onyx serve json->markdown format

### ⚙️ Miscellaneous Tasks

- Add more events to automatically trigger release
- Turn off windows release for now
- Bump version

## [0.1.10] - 2024-11-28

### 🚀 Features

- Support building windows binary

### ⚙️ Miscellaneous Tasks

- Bump version

## [NightlyBuild_2024.11.28.run_104] - 2024-11-27

### 🐛 Bug Fixes

- Revert back to ubuntu latest
- Remove search files feature
- Resolve code review
- Symlink should not be replaced by folder

### 💼 Other

- Try building with older ubuntu version

### ⚙️ Miscellaneous Tasks

- Bump version

## [NightlyBuild_2024.11.27.run_101] - 2024-11-27

### 🚀 Features

- Use vendored native-tls so we dont mess with local ssl on different linux systems (#97)

### 🐛 Bug Fixes

- Correct filename casing for banner.png

### 💼 Other

- Typo with cargo zig build

### 📚 Documentation

- Add docs for the different workflow types
- Update basic command list to indicate you have to be in project repo
- Update config docs to new config style

## [NightlyBuild_2024.11.27.run_99] - 2024-11-26

### 🚀 Features

- Support sequential loop and formatter
- Support nested loop
- Turn sequential output into vec
- Improve j2 context to provide better templating

### ⚙️ Miscellaneous Tasks

- Add dist/.gitkeep

## [NightlyBuild_2024.11.26.run_97] - 2024-11-25

### 🐛 Bug Fixes

- Resolve conflic

### 🚜 Refactor

- Simplify condition checks and improve code readability (#83)

## [NightlyBuild_2024.11.25.run_96] - 2024-11-24

### 🐛 Bug Fixes

- Fmt and remove unuse code

## [0.1.6] - 2024-11-22

### 🚀 Features

- Add legacy color support (close #77) (#82)

### ⚙️ Miscellaneous Tasks

- Bump version

## [NightlyBuild_2024.11.21.run_91] - 2024-11-20

### ⚙️ Miscellaneous Tasks

- Remove fake streaming (#74)
- Enable cargo check to run on main
- Fix missing repo token

## [NightlyBuild_2024.11.20.run_88] - 2024-11-19

### 🚀 Features

- Truncate to max 100 rows by default

### 🐛 Bug Fixes

- Use comfy_table
- Use comfy_table
- Use comfy_table

## [NightlyBuild_2024.11.19.run_87] - 2024-11-18

### 🐛 Bug Fixes

- Support all type
- Format

## [NightlyBuild_2024.11.18.run_86] - 2024-11-15

### 🐛 Bug Fixes

- Format

### 📚 Documentation

- Create first draft of guide on contributing to documentation

### ⚙️ Miscellaneous Tasks

- Add linters to commit stage
- Fix db location, reduce streaming delay (#71)

## [0.1.4] - 2024-11-15

### 🚀 Features

- Remove makefile and use turbo

### 💼 Other

- Remove mv folder
- Update onyx version to 0.1.4
- Remove pnpm cache setup from release workflow

### 📚 Documentation

- Update readme

## [NightlyBuild_2024.11.15.run_81] - 2024-11-14

### ⚙️ Miscellaneous Tasks

- Run db migration at startup (#66)

## [0.1.3] - 2024-11-14

### 🐛 Bug Fixes

- Web-app dist in binary
- Web-app dist in binary
- Web-app dist in binary (#64)
- Web-app dist in binary
- Remove unuse code
- Add fallback to index.html web-app

### ⚙️ Miscellaneous Tasks

- Use 3rd party action for some dependencies
- Cache pnpm
- Change repo name for onyx core to onyx
- Bump version
- Fix syntax error in release dep install

## [0.1.2] - 2024-11-13

### 🚀 Features

- Add agent updated_at, fix conversation not found
- Agent updated at
- Dynamic based on the local time

### 🐛 Bug Fixes

- Format
- Conflict
- Conflict
- Format
- Conversation not found
- Default env
- Default database

### ⚙️ Miscellaneous Tasks

- Anchor sql look up in data path (#59)
- Sort by updated_at
- Bump version

## [0.1.1] - 2024-11-13

### 🚀 Features

- Support conversation with agents

### ⚙️ Miscellaneous Tasks

- Ignore db file
- Return is_human and created_at
- Stream question answer object
- Cargo
- Stream question first

## [NightlyBuild_2024.11.13.run_73] - 2024-11-12

### 🚀 Features

- Support path expansion for project_path of defaults (#57)

### 💼 Other

- Switch to nightly build instead

### 📚 Documentation

- Enhance quickstart documentation and refresh brand assets (#58)

## [AutoBuild_2024.11.12.run_72] - 2024-11-12

### 💼 Other

- Update script

## [AutoBuild_2024.11.12.run_71] - 2024-11-12

### 🚜 Refactor

- Ensure that install dir work

## [AutoBuild_2024.11.12.run_69] - 2024-11-12

### 📚 Documentation

- Edit command for quickstart
- Update instruction

## [AutoBuild_2024.11.12.run_67] - 2024-11-12

### 🚀 Features

- Support execute_sql step

### 🐛 Bug Fixes

- Naming

### 🚜 Refactor

- Dont override config file if it has already existed

## [AutoBuild_2024.11.12.run_65] - 2024-11-12

### 🐛 Bug Fixes

- Script should support arm mac

### 🚜 Refactor

- Avoid using sudo

### 📚 Documentation

- Doc improvements (#52)

## [AutoBuild_2024.11.11.run_63] - 2024-11-11

### 🐛 Bug Fixes

- Credentials for github action

### ⚙️ Miscellaneous Tasks

- Replace public release job
- Temporarily allow edited event to trigger build
- Allow workflow public release to run manually
- Set owner and repositories in app token

## [AutoBuild_2024.11.11.run_57] - 2024-11-11

### 🚀 Features

- Stream answer

### ⚙️ Miscellaneous Tasks

- Update buffered
- Format code
- Fmt

## [0.1.0] - 2024-11-11

### 🐛 Bug Fixes

- Remove path when checking out
- Move git config upward

### ⚙️ Miscellaneous Tasks

- Separate public release into another workflow
- Update release event type to "published"
- Relax tagging scheme so more tags can be grouped together
- Set gh token for github public release
- Adjust concurrency key
- Set github user name for a successful tag
- Use a different cache key for better hit
- Fmt
- Bump version and push tag using formal actions

### 🚀 Features

- Add list agents api

### 💼 Other

- Use app token for release workflow

### 🚜 Refactor

- Simplify CI workflow steps for formatting and linting
- Release.yaml to improve concurrency and cancel-in-progress behavior

### 📚 Documentation

- Core workflow (#30)
- Add content to Ollama and Open AI (#36)

## [AutoBuild_2024.11.07_14-06] - 2024-11-07

### 🚀 Features

- Implement vector search
- Reorganize config
- Scaffolding app
- Support ollama
- Rearrange llm configuration and update defaults config
- Add workflow execution
- Implement generic tool types
- Implement onyx validate
- Move system instructions to upper level
- Change scope -> data and support multiple paths
- Add basic retry support
- Show dir project
- Sort dir
- Install script

### 🐛 Bug Fixes

- Resolve conflic
- Bugs
- Merge main
- Merge main
- Expected reference to Pathbuf, found Pathbuf error; also some auto-linting
- Dialect isn't valid (should be type)
- Add pnpm install to ensure deps are installed
- Add version for rust toolchain
- Install libssl dev on some platforms
- Build commands
- Dependency installation
- Openssl for linux
- Error: variant `Text` is never constructed
- Missing typo
- Build
- Merge main
- Merge main

### 💼 Other

- Move web app dist to the right folder
- Enable cargo check to run with the right tools
- Move web-app/dist directory during CI workflow
- Move web-app/dist directory during CI workflow
- Add support for cross-compilation in release workflow
- Ensure release is always tagged

### 🚜 Refactor

- Remove unused GitHub workflows
- Update .cargo/config.toml
- Update Rust Toolchain Target and install script
- Simplify release binary naming convention
- Remove unused GitHub workflows and update installation script
- Add changesets workflow for release.yaml
- Update .cargo/config.toml and Cargo.toml
- Update release.yaml and README.md
- Remove unused GitHub workflows and update installation script
- Update installation script URL in README.md
- Update CI workflow to include pull request types and concurrency
- Update CI workflow

### 📚 Documentation

- Update readme for internal setup and instructions (#15)
- Update theme and get started documentation (#19)
- License onyx to agpl v3
- Add explanation for protoc and remove windows

### 🎨 Styling

- Apply auto format to code
- Apply clippy fix
- Apply fmt fix

### ⚙️ Miscellaneous Tasks

- Update README
- Misc on error message and prompt
- Hardcode db connection string
- Improve output format prompt
- Cleanup println
- Add codeowners and precommit config
- Check build, run test and lint format
- Add change detection
- Use change detection in onyx ci
- Add continuous release workflow and configuration
- Add build frontend assets
- Lock pnpm ver
- Minor improvements to ci and release workflow
- Add auto release workflow
- Replace action setup of pnpm with yaml string
- Add package json file
- Temporarily disable cross compilation
- Use latest ubuntu for better version of protoc
- Use latest ubuntu for better version of protoc
- Remove cargo build in ci because it might takes too long
- Run cargo build with warnings only
- Treat warning as warnings
- Allow build to succeed with warnings
- Demo
- Temporarily cutdown some steps for CI because it takes too long
- Add concurrency control for release step

## [0.0.0a0] - 2024-09-25

### 🚀 Features

- Include db schemas into instruction
- Integrate tools
- Improve aesthetic

### 🐛 Bug Fixes

- Remove deprecated model
- Typo in default config file
- Limit content passed to openai

<!-- generated by git-cliff -->
