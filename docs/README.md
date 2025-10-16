# Running the docs

## Local Development Setup

This documentation site is built with [Mintlify](https://mintlify.com/). To run the docs locally for development and preview changes:

### Prerequisites

1. Install the Mintlify CLI:
   ```bash
   npm install -g mintlify
   ```

### Running the Development Server

1. Navigate to the docs directory (this directory):
   ```bash
   cd docs
   ```

2. Start the local development server:
   ```bash
   mint dev
   ```

3. The docs will be available at `http://localhost:3000` by default. The development server will automatically reload when you make changes to any documentation files.

### Project Structure

- `docs.json` - Main configuration file for Mintlify
- `*.mdx` files - Documentation pages written in MDX (Markdown + React components)
- `images/` - Static assets and images
- `logo/` - Brand assets (light/dark logos)
- Navigation structure is defined in `docs.json` under the `navigation` section

### Making Changes

1. Edit any `.mdx` file in the appropriate directory
2. The development server will automatically reflect your changes
3. For navigation changes, update the `docs.json` configuration file

### Deployment

The documentation is automatically deployed when changes are pushed to the main branch. No manual deployment steps are required for production updates.
