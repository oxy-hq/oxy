# Oxy Enterprise Smoke Test
# Run with: hurl --test tests/smoke/smoke.hurl
# Local project ID is always 00000000-0000-0000-0000-000000000000

# Health endpoint (includes database connectivity check)
GET http://localhost:3000/api/health
HTTP 200
[Asserts]
header "Content-Type" contains "application/json"
jsonpath "$.status" == "healthy"
jsonpath "$.database.connected" == true
duration < 5000

# Version endpoint
GET http://localhost:3000/api/version
HTTP 200
[Asserts]
header "Content-Type" contains "application/json"
jsonpath "$.service" == "oxy"
duration < 1000

# Project status (validates config)
GET http://localhost:3000/api/00000000-0000-0000-0000-000000000000/status?branch=main
HTTP 200
[Asserts]
header "Content-Type" contains "application/json"
jsonpath "$.is_config_valid" == true
duration < 5000

# Agents endpoint
GET http://localhost:3000/api/00000000-0000-0000-0000-000000000000/agents?branch=main
HTTP 200
[Asserts]
header "Content-Type" contains "application/json"
duration < 5000

# Databases endpoint
GET http://localhost:3000/api/00000000-0000-0000-0000-000000000000/databases?branch=main
HTTP 200
[Asserts]
header "Content-Type" contains "application/json"
duration < 5000

# Workflows endpoint
GET http://localhost:3000/api/00000000-0000-0000-0000-000000000000/workflows?branch=main
HTTP 200
[Asserts]
header "Content-Type" contains "application/json"
duration < 5000

# App endpoint
GET http://localhost:3000/api/00000000-0000-0000-0000-000000000000/app?branch=main
HTTP 200
[Asserts]
header "Content-Type" contains "application/json"
duration < 5000

# Threads endpoint
GET http://localhost:3000/api/00000000-0000-0000-0000-000000000000/threads?page=1&limit=50
HTTP 200
[Asserts]
header "Content-Type" contains "application/json"
duration < 5000
