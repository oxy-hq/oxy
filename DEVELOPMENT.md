# Development

- [Development](#development)
  - [Clone This Repository](#clone-this-repository)
  - [Install Rust \& Node](#install-rust--node)
  - [Install Project Dependencies](#install-project-dependencies)
  - [Test Your Installation](#test-your-installation)
  - [Other Commands](#other-commands)
    - [Manual Testing Oxy Integration with Different Databases](#manual-testing-oxy-integration-with-different-databases)
  - [Known Issues](#known-issues)
  - [OxyPy Requirements](#oxypy-requirements)

## Clone This Repository

```sh
git clone git@github.com:oxy-hq/oxy.git
cd oxy
```

## Install Rust & Node

| :zap: **You are responsible for your own setup if you decide to not follow the instructions below.** |
| ---------------------------------------------------------------------------------------------------- |

Install Rust by following the official guide [here](https://www.rust-lang.org/tools/install).

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Add Cargo to your path by following the instructions at the end of the installation. Or:

```bash
# Detect the current shell
current_shell=$(basename "$SHELL")

# Append the PATH export line to the appropriate shell profile
case "$current_shell" in
  bash)
    echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
    source ~/.bashrc
    ;;
  zsh)
    echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
    source ~/.zshrc
    ;;
  *)
    echo "Unsupported shell: $current_shell"
    ;;
esac
```

Install Node by following the official guide [here](https://nodejs.org/en/download/) or using mise.
We recommend using the LTS version.
We recommend using mise to install Node and manage versions, to isolate the Node environment from the system.

```sh
# Using mise is a recommended way to install Node and manage versions
curl https://mise.run | sh


case $SHELL in
  *bash)
    echo 'eval "$(~/.local/bin/mise activate bash)"' >> ~/.bashrc
    source ~/.bashrc
    ;;
  *zsh)
    echo 'eval "$(~/.local/bin/mise activate zsh)"' >> ~/.zshrc
    source ~/.zshrc
    ;;
esac

# inside this repo
mise install
```

## Install Project Dependencies

After Rust and Node.js are installed, install project dependencies:

```sh
corepack enable && corepack prepare --activate
pnpm install

# this will just build the CLI
cargo build

# this will just build the frontend
pnpm -C web-app build

# a shortcut for building both the CLI and the frontend
# pnpm build
```

> **Important:** Ensure you have the necessary keys and environment variables set up before running your first query.

## Test Your Installation

```sh
cargo run -- --help
```

or you can start following our [quickstart guide](https://docs.oxy.tech/docs/quickstart).

## Other Commands

Please check out the docs folder for more commands and examples.

### Manual Testing Oxy Integration with Different Databases

We use Docker Compose to spin up database containers to create and supply test data. Ensure Docker and Docker Compose are installed on your system.

1. Start the required database containers:

   ```sh
   cd examples # make sure the docker compose you are trying to stand up is inside examples folder
   docker-compose up -d
   ```

2. Verify the containers are running:

   ```sh
   docker ps
   ```

3. Inside `examples` folder, you will find an agent for each database. Run the agent with the following command:

   ```sh
   cargo run -- run agents/<database>.agent.yml
   ```

   Replace `<database>` with the database you want to test.

4. To stop the containers after testing:

   ```sh
   docker-compose down
   ```

## Known Issues

- Tests when running in parallel (default) messed up terminal indentation.

## OxyPy Requirements

Ensure that the right Python version is in your path (oxyuses python3.11.6). There are many ways to install Python, here we recommend a few ways:

```sh
# Using mise is the best for any architecture
curl https://mise.run | sh


case $SHELL in
  *bash)
    echo 'eval "$(~/.local/bin/mise activate bash)"' >> ~/.bashrc
    source ~/.bashrc
    ;;
  *zsh)
    echo 'eval "$(~/.local/bin/mise activate zsh)"' >> ~/.zshrc
    source ~/.zshrc
    ;;
esac

mise install python 3.11.6

mise use python@3.11 --global

```

```sh
# Using brew
brew install python@3.11
brew link python@3.11
```

Poetry is a Python package manager that we use to manage Python dependencies. Install Poetry by following the official guide [here](https://python-poetry.org/docs/). Or you can install it with pip:

```sh
pip install poetry
```

## Development Guide

## Testing Authentication with Seeded Users

This guide shows how to test the new Google IAP authentication system using seeded test users.

### 1. Setup Database & Run Migrations

First, set up your database URL and run the migrations:

```bash
# Set SQLite database URL
export DATABASE_URL="sqlite://./oxy.db"

# Run migrations to create users and threads tables
cargo run -p migration -- up
```

### 2. Seed Test Data

Use the new `seed` command to populate your database with test users:

```bash
# Create test users
cargo run seed users

# Create sample threads for users
cargo run seed threads

# Or do both at once
cargo run seed full

# Clear all test data when done
cargo run seed clear
```

### 3. Test Users Created

The seeding system creates these test users:

| Email | Name | Role |
|-------|------|------|
| `alice.smith@company.com` | Alice Smith | Test User |
| `bob.johnson@company.com` | Bob Johnson | Test User |
| `carol.williams@company.com` | Carol Williams | Test User |
| `david.brown@company.com` | David Brown | Test User |
| `eva.davis@company.com` | Eva Davis | Test User |
| `frank.miller@company.com` | Frank Miller | Test User |
| `grace.wilson@company.com` | Grace Wilson | Test User |
| `henry.moore@company.com` | Henry Moore | Test User |
| `iris.taylor@company.com` | Iris Taylor | Test User |
| `jack.anderson@company.com` | Jack Anderson | Test User |

### 4. Testing Authentication

#### Method 1: Default Test User (Alice)
In development mode, if no authentication headers are provided, the system defaults to `alice.smith@company.com`:

```bash
# Start the server
cargo run serve

# Test API - will use Alice by default
curl http://localhost:3000/api/user
curl http://localhost:3000/api/threads
```

#### Method 2: Specify Test User via Header
Use the `X-Test-User` header to test with different users:

```bash
# Test as Bob
curl -H "X-Test-User: bob.johnson@company.com" http://localhost:3000/api/user

# Test Bob's threads
curl -H "X-Test-User: bob.johnson@company.com" http://localhost:3000/api/threads

# Test as Carol
curl -H "X-Test-User: carol.williams@company.com" http://localhost:3000/api/threads
```

#### Method 3: Web Interface Testing
You can also test via the web interface by opening your browser's developer tools and setting a header:

1. Open Developer Tools → Network tab
2. Right-click and "Copy as cURL"
3. Add `-H "X-Test-User: email@company.com"` to test different users

### 5. Verify User Isolation

Test that users can only see their own threads:

```bash
# Alice's threads
curl -H "X-Test-User: alice.smith@company.com" http://localhost:3000/api/threads

# Bob's threads (different set)
curl -H "X-Test-User: bob.johnson@company.com" http://localhost:3000/api/threads

# Create a thread as Alice
curl -X POST -H "Content-Type: application/json" \
  -H "X-Test-User: alice.smith@company.com" \
  -d '{"title": "Test Thread", "input": "Test question", "source": "test.sql", "source_type": "sql"}' \
  http://localhost:3000/api/threads

# Verify Bob cannot see Alice's new thread
curl -H "X-Test-User: bob.johnson@company.com" http://localhost:3000/api/threads
```

### 6. Database Inspection

You can also inspect the database directly:

```bash
# View all users
sqlite3 oxy.db "SELECT id, email, name, created_at FROM users;"

# View threads with user info
sqlite3 oxy.db "
SELECT 
  t.title, 
  t.input, 
  u.email as user_email,
  t.created_at 
FROM threads t 
JOIN users u ON t.user_id = u.id 
ORDER BY t.created_at DESC;
"

# Count threads per user
sqlite3 oxy.db "
SELECT 
  u.email, 
  COUNT(t.id) as thread_count 
FROM users u 
LEFT JOIN threads t ON u.id = t.user_id 
GROUP BY u.id, u.email;
"
```

### 7. Production Deployment

For production with Google Cloud Run + IAP:

1. **Set IAP Audience**: `export IAP_AUDIENCE="your-project-number"`
2. **Deploy to Cloud Run** with IAP enabled
3. **Configure IAP** for your organization's Google Workspace domain
4. **Remove test headers** - the system will use real IAP JWT tokens

### 8. Debugging Authentication

Enable debug logging to see authentication flow:

```bash
export RUST_LOG=oxy=debug,tower_http=debug
cargo run serve
```

Look for log messages like:
- `"Using test token for email: alice.smith@company.com"`
- `"Created new user: alice.smith@company.com"`
- `"JWT validation failed: ..."`

### 9. Cleanup

When you're done testing:

```bash
# Clear all test data
cargo run seed clear

# Or manually delete the database
rm oxy.db
```

## API Endpoints

### Authentication Required
- `GET /api/user` - Get current user info
- `PUT /api/user` - Update user profile
- `GET /api/threads` - Get user's threads
- `POST /api/threads` - Create new thread
- `GET /api/threads/{id}` - Get specific thread
- `DELETE /api/threads/{id}` - Delete thread

### Public (Optional Auth)
- `GET /api/agents` - List agents
- `POST /api/ask` - Ask agents questions

All authenticated endpoints will return 401 Unauthorized without proper authentication headers in production, or will use the default test user in development mode.
