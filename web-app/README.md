# Vite + React + TypeScript + SWC

This project combines cutting-edge technologies to create a powerful, efficient, and developer-friendly web application development environment.

- [Vite + React + TypeScript + SWC](#vite--react--typescript--swc)
  - [Features and Rationale](#features-and-rationale)
    - [Core Technologies](#core-technologies)
    - [UI and Styling](#ui-and-styling)
    - [Code Quality and Consistency](#code-quality-and-consistency)
    - [Git Workflow and Commit Quality](#git-workflow-and-commit-quality)
    - [State Management and Data Fetching](#state-management-and-data-fetching)
    - [Routing](#routing)
  - [Prerequisites](#prerequisites)
  - [Getting Started](#getting-started)
  - [Available Scripts](#available-scripts)
  - [Configuration](#configuration)
  - [Learn More](#learn-more)

## Features and Rationale

### Core Technologies

1. **React.js**

2. **TypeScript**

3. **Vite**

   - A fast, modern frontend build tool.

4. **SWC (Speedy Web Compiler)**
   - A super-fast JavaScript/TypeScript compiler written in Rust.

### UI and Styling

5. **Radix Primitives**

   - Unstyled, accessible components for building high-quality design systems and web apps.

6. **Panda CSS**
   - A lightweight, flexible, and customizable CSS-in-JS solution.

### Code Quality and Consistency

7. **ESLint**

   - Identifies and reports on patterns in JavaScript/TypeScript code.
   - Notable plugins and extends:
     - eslint-config-airbnb: Popular style guide for React projects
     - eslint-config-airbnb-typescript: TypeScript support for Airbnb style guide
     - eslint-plugin-import: Helps validate proper imports
     - eslint-plugin-jsx-a11y: Checks for accessibility issues in JSX
     - eslint-plugin-react: React specific linting rules
     - eslint-plugin-react-hooks: Enforces Rules of Hooks
     - eslint-plugin-unicorn: Additional helpful rules and best practices
     - @typescript-eslint/eslint-plugin: TypeScript-specific linting rules

8. **Prettier**

### Git Workflow and Commit Quality

9. **Husky**

10. **Commitlint**

    - Checks if your commit messages meet the conventional commit format.

11. **Lint-staged**

### State Management and Data Fetching

12. **React Query**

    - A powerful library for managing server state in React applications.

13. **Zustand**
    - A small, fast, and scalable state management solution.

### Routing

14. **React Router 6**
    - A collection of navigational components for React applications.

## Prerequisites

- Node.js version 20.0.0 or higher
- pnpm version 8.0.0 or higher

## Getting Started

To get started with this project, follow these steps:

1. Ensure you have the correct Node.js version installed:

   ```
   node --version
   ```

   If you need to update or switch Node versions, we recommend using [nvm](https://github.com/nvm-sh/nvm) or [volta](https://volta.sh/).

2. Clone the repository
3. Install dependencies: `pnpm install`
4. Start the development server: `pnpm dev`


## Available Scripts

- `pnpm dev`: Starts the development server
- `pnpm build`: Builds the project for production
- `pnpm lint`: Runs ESLint
- `pnpm format`: Runs Prettier
- `pnpm typecheck`: Runs TypeScript type checking


## Configuration

This project uses various configuration files:

- `vite.config.ts`: Vite configuration
- `.eslintrc.js`: ESLint configuration
- `.prettierrc`: Prettier configuration
- `tsconfig.json`: TypeScript configuration
- `commitlint.config.js`: Commitlint configuration

## Learn More

To learn more about the technologies used in this project, check out the following resources:

- [Vite Documentation](https://vitejs.dev/)
- [React Documentation](https://reactjs.org/)
- [TypeScript Documentation](https://www.typescriptlang.org/)
- [Radix Primitives Documentation](https://www.radix-ui.com/)
- [Panda CSS Documentation](https://panda-css.com/)
- [ESLint Documentation](https://eslint.org/)
- [Prettier Documentation](https://prettier.io/)
- [Husky Documentation](https://typicode.github.io/husky/)
- [Commitlint Documentation](https://commitlint.js.org/)
- [Lint-staged Documentation](https://github.com/okonet/lint-staged)
- [SWC Documentation](https://swc.rs/)
- [React Query Documentation](https://react-query.tanstack.com/)
- [Zustand Documentation](https://github.com/pmndrs/zustand)
- [React Router 6 Documentation](https://reactrouter.com/)
