import remarkPresetLintConsistent from "remark-preset-lint-consistent";
import remarkPresetLintRecommended from "remark-preset-lint-recommended";

const remarkConfig = {
  plugins: [
    remarkPresetLintConsistent, // Check that markdown is consistent.
    remarkPresetLintRecommended, // Few recommended rules.
    // Generate a table of contents in `## Contents`
  ],
  settings: {
    bullet: "-",
    bulletOther: "*",
    // See <https://github.com/remarkjs/remark/tree/main/packages/remark-stringify> for more options.
  },
};

export default remarkConfig;
