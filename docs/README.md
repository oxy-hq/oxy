# Oxy Documentation

This folder serves as the documentation for Oxy usage. It is built using Mintlify.

The build happens automatically as part of the Oxy Content CI CD Pipeline. It does not happen within this repository. For more information, please visit [the Oxy Content repository](https://github.com/oxy-hq/oxy-content).

The reason we keep the folder here is to allow lower context switching for developers working on both the codebase and the documentation.

## Instruction

- To render the docs on your local machine, run `pnpx mint dev`
- To deploy the doc to `oxy.tech/changelog`, trigger the workflow in the [Oxy Content repository](https://github.com/oxy-hq/oxy-content)
- To edit the docs, simply modify the `.mdx` files in this folder. Refer to [Mintlify Documentation](https://docs.mintlify.com/docs/getting-started/introduction) for more details on how to write docs using Mintlify

## Internal develolment docs

- Should not live in this folder, but should go inside `internal-docs/` instead
