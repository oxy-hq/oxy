#!/usr/bin/env bash
#
# Database Cleanup Script - Drops and recreates the database
#
# Usage: ./scripts/cleanup-database.sh
#

set -e

# Load .env file
if [ -f .env ]; then
    export $(grep -v '^#' .env | grep -v '^$' | xargs)
fi

# Check if OXY_DATABASE_URL is set
if [ -z "$OXY_DATABASE_URL" ]; then
    echo "Error: OXY_DATABASE_URL is not set"
    exit 1
fi

# Extract database details from URL
# Format: postgresql://user:password@host:port/database
DB_URL_REGEX="postgresql://([^:]+):([^@]+)@([^:]+):([^/]+)/(.+)"
if [[ $OXY_DATABASE_URL =~ $DB_URL_REGEX ]]; then
    DB_USER="${BASH_REMATCH[1]}"
    DB_PASSWORD="${BASH_REMATCH[2]}"
    DB_HOST="${BASH_REMATCH[3]}"
    DB_PORT="${BASH_REMATCH[4]}"
    DB_NAME="${BASH_REMATCH[5]}"
else
    echo "Error: Invalid database URL format"
    exit 1
fi

echo "WARNING: This will DELETE ALL DATA in database '$DB_NAME'!"
echo "Press Ctrl+C to cancel, or Enter to continue..."
read

export PGPASSWORD="$DB_PASSWORD"

echo "Dropping database..."
psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d postgres -c "DROP DATABASE IF EXISTS \"$DB_NAME\";"

echo "Creating database..."
psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d postgres -c "CREATE DATABASE \"$DB_NAME\";"

echo "Running migrations..."
cd crates/migration
cargo run -- up
cd ../..

echo "✓ Database cleanup completed!"
