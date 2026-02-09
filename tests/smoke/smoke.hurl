# Oxy Enterprise Smoke Test
# Run with: hurl --test tests/smoke/smoke.hurl

# Health endpoint
GET http://localhost:3000/health
HTTP 200
[Asserts]
duration < 1000

# Index page
GET http://localhost:3000/
HTTP 200
[Asserts]
duration < 2000

# API - Apps endpoint
GET http://localhost:3000/api/apps
HTTP 200
[Asserts]
duration < 5000
header "Content-Type" contains "application/json"

# API - Databases endpoint (may return empty array)
GET http://localhost:3000/api/databases
HTTP 200
[Asserts]
duration < 5000
