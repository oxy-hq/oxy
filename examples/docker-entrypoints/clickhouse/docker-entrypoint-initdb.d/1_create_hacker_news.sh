#!/bin/bash
set -e
clickhouse client -n <<-EOSQL
CREATE TABLE default.hacker_news_materialised (
    id Int64,
    deleted Int64,
    type String,
    by String,
    time DateTime,
    text String,
    dead Int64,
    parent Int64,
    poll Int64,
    kids Array(String),
    url String,
    score Int64,
    title String,
    parts Array(String),
    descendants Int64
) ENGINE = MergeTree()
ORDER BY (type, id);

CREATE TABLE default.hacker_news_subset (
    id Int64,
    text String,
    by String
) ENGINE = MergeTree()
ORDER BY id;

INSERT INTO default.hacker_news_materialised (id, deleted, type, by, time, text, dead, parent, poll, kids, url, score, title, parts, descendants) VALUES 
(1, 0, 'story', 'user1', now(), 'Example text 1', 0, 0, 0, [], 'http://example.com/1', 10, 'Example Title 1', [], 0),
(2, 0, 'comment', 'user2', now(), 'Example text 2', 0, 1, 0, [], '', 0, '', [], 0),
(3, 0, 'story', 'user3', now(), 'Example text 3', 0, 0, 0, [], 'http://example.com/3', 15, 'Example Title 3', [], 0),
(4, 0, 'comment', 'user4', now(), 'Example text 4', 0, 3, 0, [], '', 0, '', [], 0),
(5, 0, 'story', 'user5', now(), 'Example text 5', 0, 0, 0, [], 'http://example.com/5', 20, 'Example Title 5', [], 0),
(6, 0, 'comment', 'user6', now(), 'Example text 6', 0, 5, 0, [], '', 0, '', [], 0),
(7, 0, 'story', 'user7', now(), 'Example text 7', 0, 0, 0, [], 'http://example.com/7', 25, 'Example Title 7', [], 0),
(8, 0, 'comment', 'user8', now(), 'Example text 8', 0, 7, 0, [], '', 0, '', [], 0),
(9, 0, 'story', 'user9', now(), 'Example text 9', 0, 0, 0, [], 'http://example.com/9', 30, 'Example Title 9', [], 0),
(10, 0, 'comment', 'user10', now(), 'Example text 10', 0, 9, 0, [], '', 0, '', [], 0);

INSERT INTO default.hacker_news_subset (id, text, by) VALUES 
(1, 'Example text 1', 'user1'),
(2, 'Example text 2', 'user2'),
(3, 'Example text 3', 'user3'),
(4, 'Example text 4', 'user4'),
(5, 'Example text 5', 'user5'),
(6, 'Example text 6', 'user6'),
(7, 'Example text 7', 'user7'),
(8, 'Example text 8', 'user8'),
(9, 'Example text 9', 'user9'),
(10, 'Example text 10', 'user10');
EOSQL
