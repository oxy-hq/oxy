import remarkPresetLintConsistent from "remark-preset-lint-consistent";
import remarkPresetLintRecommended from "remark-preset-lint-recommended";
import remarkGfm from "remark-gfm";
const remarkConfig = {
  plugins: [
    remarkPresetLintConsistent, // Check that markdown is consistent.
    remarkPresetLintRecommended, // Few recommended rules.
    remarkGfm,
  ],
  settings: {
    bullet: "-",
    bulletOther: "*",
    // See <https://github.com/remarkjs/remark/tree/main/packages/remark-stringify> for more options.
  },
};

export default remarkConfig;
