# Release guideline

- [Release guideline](#release-guideline)
  - [Release schedule](#release-schedule)
  - [Release manager](#release-manager)
  - [Release process](#release-process)
  - [Test plan](#test-plan)
    - [New features and bug fixes](#new-features-and-bug-fixes)
    - [Regression testing](#regression-testing)
    - [Core features](#core-features)

## Release schedule

- **Every week** on **Thursday** at **10:00 AM** (UTC+7) for private release
- After private release has been verified, public release will be scheduled on the following **Monday** at **10:00 AM** (UTC+7)
- Should there be any issues with the private release, the public release will be postponed until the issues are resolved

## Release manager

The duties of the release manager include:

- Verify that the release notes and assets are correct
- Verify that the release has been successfully deployed
- Test the release and provide feedback
- Trigger the public release workflow
- Verify that the public release has been successfully deployed
- Coordinate with the team to address any issues that arise during the release process
- Communicate with the team (or customers) about the status of the release in case of any delays
- Update documentation and other relevant information as needed

## Release process

**Private release**:

- The release manager merges the Release Pull Request into the main branch. Such pull request is created by a bot and will look like [this](https://github.com/onyx-hq/onyx/pull/196), with only changes to version numbers and release notes.
- The release manager then waits for the build pipeline to succeed and a private release is ready to be tested.
  - Because the build pipeline takes a while to complete, if the release manager knows how to build the project locally, they can start the build pipeline and then build the project locally to save time.
- The release manager tests the release and provide feedback
  - Scope of testing: all features and bug fixes in the release, plus any core features that are frequently used
  - The release manager should traceback changes by pull request to see reproduction steps and testing plan if needed
- Dedicated developer addresses release manager's feedback and update the release by creating a new pull request
  - These pull requests should have higher priority than other tasks, and should be merged before changes that are not related to the release
- The release manager merges the Release Pull Request into the main branch once all feedbacks have been addressed
- The release manager retests the release and verifies that the issues have been resolved
**Public release**:
- The release manager will trigger the [public release workflow](https://github.com/onyx-hq/onyx/actions/workflows/public-release.yaml) by clicking on "Run workflow" in the Actions tab
- After the public release workflow has completed, the release manager will verify that the release has been successfully deployed by visiting [onyx-public-releases](https://github.com/onyx-hq/onyx-public-releases/releases)
- A new release should be ready there and marked as `draft`. The release manager will verify that the release notes and assets are correct and then publish the release by editing the release and clicking on "Publish release"

## Test plan

### New features and bug fixes

Every new feature and bug fix is linked to a pull request. The release manager should be able to trace back to the pull request to see the testing plan and reproduction steps. The release manager should also be able to see the test results in the pull request. In cases where the instruction is not clear or questions arise, the release manager should ask the developer for clarification.

### Regression testing

The release manager should perform regression testing on core features that are frequently used. The release manager should also perform regression testing on features that have been changed in the release. The release manager should be able to trace back to the pull request to see the testing plan and reproduction steps. In cases where the instruction is not clear or questions arise, the release manager should ask the developer for clarification.

### Core features

- **Testing requirements**:
  - Inside the project directory, `cd examples` to change working directory to the examples directory
  - The version that needs to be tested should be installed or built locally with `cargo build --release`
  - OPENAI_API_KEY should be set in the environment variables
  - Big Query credentials should be saved into a file called `bigquery-sample.key` in examples directory
  - Credentials like OPENAI_API_KEY and Big Query credentials can be found in [1password](https://start.1password.com/open/i?a=IMJXCOHRCZESPKAZF4EKLLFKVM&v=7a3p4o4szzhubnctexpd4m32ja&i=uhdtkgd3vi7r3opizwuoujnuc4&h=onyxint.1password.com)

- **Description**: The `onyx run` command is used to run the Onyx server
- **Testing plan**:
  - [x] Check onyx run with an agent and bigquery by running `onyx run agents/default.agent.yml "how many users are there in the database"` *(automated since #219)*
  - [x] Check onyx run with semantic information by running `onyx run agents/semantic_model.agent.yml "how many users are there in the database"` *(automated since #219)*
    - [ ] Additionally, check the difference between the two outputs of `onyx run agents/default.agent.yml "how many property_grouping"` and `onyx run agents/semantic_model.agent.yml "how many property_grouping"`
  - [x] Check onyx run workflow capabilities by running `onyx run workflows/table_values.workflow.yml` and see if workflow can be run successfully with colored output *(automated since #219)*
  - [x] Check onyx anonymization capabilities with workflow by running `onyx run workflows/anonymize.workflow.yml` and see if the workflow can be run successfully with a lot of mentions of `comnpany` and `feedback` while the report name is still `Responses to the survey by organizations` *(automated since #219)*
  - [x] Check onyx run workflow capabilities with loops sequential and formatter by running `onyx run workflows/survey_responses.workflow.yml` and see if workflow can be run successfully with colored output *(automated since #219)*

- **Description**: The `onyx serve` command is used to run the Onyx server
  - [x] Check onyx serve by running `onyx serve` and see whether the server is running as well as `localhost:3000` is accessible from the browser. *(automated since #219)*
    - [ ] Additionally, perform some chatting in the UI with the same questions to the agents such as `how many users`

- **Description**: The `onyx validate` command is used to validate the configuration file
  - [x] Check onyx validate by running `onyx validate` after changing some fields to a wrong key (like `warehouses` to `warehouse`) in `config.yml` *(automated since #219)*

- **Description**: The `onyx list-tables` and `onyx list-datasets` commands are used to list tables and datasets in the default warehouse
  - [x] Check onyx discovery capabilities by running `onyx list-tables` or `onyx list-datasets` *(automated since #219)*