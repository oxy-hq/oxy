export default {
  extends: ["@commitlint/config-conventional"],
  rules: {
    "header-max-length": [2, "always", 120],
    // Disabled: commit body paragraphs don't benefit from hard wrapping —
    // modern git tooling wraps on display. Keep the header cap for scannable
    // log lines, but don't force bullet lists to break mid-sentence.
    "body-max-line-length": [0]
  }
};
