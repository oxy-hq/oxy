CREATE TABLE hacker_news (
    "id" NUMERIC NOT NULL,
    "deleted" NUMERIC,
    "type" VARCHAR NOT NULL,
    "by" VARCHAR,
    "time" TIMESTAMP,
    "text" TEXT,
    "dead" NUMERIC,
    "parent" NUMERIC,
    "poll" NUMERIC,
    "kids" TEXT [],
    "url" VARCHAR,
    "score" NUMERIC,
    "title" TEXT,
    "parts" text [],
    "descendants" NUMERIC
);
INSERT INTO hacker_news
VALUES (
        18346787,
        0,
        'comment',
        'RobAtticus',
        '2018-10-31 15:56:39.000000000',
        'We do have comparisons, but judging by their Medium read times some may not be considered &quot;quick&quot; :)<p>* Influx: <a href="https:&#x2F;&#x2F;blog.timescale.com&#x2F;timescaledb-vs-influxdb-for-time-series-data-timescale-influx-sql-nosql-36489299877" rel="nofollow">https:&#x2F;&#x2F;blog.timescale.com&#x2F;timescaledb-vs-influxdb-for-time-...</a><p>* Cassandra: <a href="https:&#x2F;&#x2F;blog.timescale.com&#x2F;time-series-data-cassandra-vs-timescaledb-postgresql-7c2cc50a89ce" rel="nofollow">https:&#x2F;&#x2F;blog.timescale.com&#x2F;time-series-data-cassandra-vs-tim...</a><p>* Mongo: <a href="https:&#x2F;&#x2F;blog.timescale.com&#x2F;how-to-store-time-series-data-mongodb-vs-timescaledb-postgresql-a73939734016" rel="nofollow">https:&#x2F;&#x2F;blog.timescale.com&#x2F;how-to-store-time-series-data-mon...</a><p>We also released a tool called Time Series Benchmark Suite (TSBS) here that someone just submitted a PR for Clickhouse: <a href="https:&#x2F;&#x2F;github.com&#x2F;timescale&#x2F;tsbs&#x2F;pull&#x2F;26" rel="nofollow">https:&#x2F;&#x2F;github.com&#x2F;timescale&#x2F;tsbs&#x2F;pull&#x2F;26</a><p>There is also this spreadsheet that compares a bunch of different time series databases, including TimescaleDB: <a href="https:&#x2F;&#x2F;docs.google.com&#x2F;spreadsheets&#x2F;d&#x2F;1sMQe9oOKhMhIVw9WmuCEWdPtAoccJ4a-IuZv4fXDHxM&#x2F;pubhtml" rel="nofollow">https:&#x2F;&#x2F;docs.google.com&#x2F;spreadsheets&#x2F;d&#x2F;1sMQe9oOKhMhIVw9WmuCE...</a><p>Hopefully some of that is useful :)',
        0,
        18346746,
        0,
        ARRAY ['18346822'],
        '',
        0,
        '',
        ARRAY []::text [],
        0
    );