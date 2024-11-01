# Vite + React + TypeScript + SWC

This project combines cutting-edge technologies to create a powerful, efficient, and developer-friendly web application development environment.

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
   - A popular JavaScript library for building user interfaces.
   - Reasons for choice:
     - Component-based architecture for reusable UI elements
     - Virtual DOM for efficient updates and rendering
     - Large ecosystem and community support
     - Excellent performance for complex UIs

2. **TypeScript**
   - Adds static typing to JavaScript, enhancing developer productivity and code quality.
   - Reasons for choice:
     - Catch errors early in development
     - Improved code readability and maintainability
     - Better IDE support and autocompletion
     - Facilitates large-scale application development

3. **Vite**
   - A fast, modern frontend build tool.
   - Reasons for choice:
     - Lightning-fast hot module replacement (HMR)
     - Out-of-the-box support for TypeScript and various frameworks
     - Optimized build process for better performance
     - Simple configuration and plugin system

4. **SWC (Speedy Web Compiler)**
   - A super-fast JavaScript/TypeScript compiler written in Rust.
   - Reasons for choice:
     - Significantly faster compilation times compared to Babel
     - Compatible with most JavaScript tools and frameworks
     - Continuously improving and gaining community support

### UI and Styling

5. **Radix Primitives**
   - Unstyled, accessible components for building high-quality design systems and web apps.
   - Reasons for choice:
     - Provides a solid foundation for custom component libraries
     - Ensures accessibility out of the box
     - Flexible and easily customizable
     - Well-documented and maintained

6. **Panda CSS**
   - A lightweight, flexible, and customizable CSS-in-JS solution.
   - Reasons for choice:
     - Type-safe styling with TypeScript integration
     - Atomic CSS approach for optimized performance
     - Easy theming and dark mode support
     - Good developer experience with autocomplete and validation

### Code Quality and Consistency

7. **ESLint**
   - Identifies and reports on patterns in JavaScript/TypeScript code.
   - Reasons for choice:
     - Enforces code style and best practices
     - Catches potential errors and bugs
     - Highly configurable to fit project needs
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
   - An opinionated code formatter.
   - Reasons for choice:
     - Ensures consistent code style across the project
     - Saves time on code formatting discussions
     - Integrates well with most editors and IDEs

### Git Workflow and Commit Quality

9. **Husky**
   - Uses Git hooks to improve your commits.
   - Reasons for choice:
     - Automates running scripts before commits or pushes
     - Ensures code quality checks are run consistently
     - Easy to set up and configure

10. **Commitlint**
    - Checks if your commit messages meet the conventional commit format.
    - Reasons for choice:
      - Enforces consistent and meaningful commit messages
      - Facilitates automated changelog generation
      - Improves project history readability

11. **Lint-staged**
    - Runs linters on pre-committed files.
    - Reasons for choice:
      - Speeds up the linting process by only checking staged files
      - Ensures only clean code is committed
      - Works well with Husky for pre-commit hooks

### State Management and Data Fetching

12. **React Query**
    - A powerful library for managing server state in React applications.
    - Reasons for choice:
      - Simplifies data fetching, caching, and state management
      - Automatic background refetching and invalidation
      - Optimistic updates for improved user experience
      - Powerful devtools for debugging

13. **Zustand**
    - A small, fast, and scalable state management solution.
    - Reasons for choice:
      - Simple and intuitive API
      - Minimal boilerplate compared to other state management libraries
      - Works well with React hooks
      - Easy to integrate with TypeScript

### Routing

14. **React Router 6**
    - A collection of navigational components for React applications.
    - Reasons for choice:
      - Declarative routing for React applications
      - Supports nested routes and layouts
      - Improved performance over previous versions
      - Well-maintained and widely adopted in the React ecosystem

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
