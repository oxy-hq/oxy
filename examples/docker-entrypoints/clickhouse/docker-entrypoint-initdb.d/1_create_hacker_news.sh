#!/bin/bash
set -e
clickhouse client -n <<-EOSQL
CREATE TABLE default.hacker_news_materialised (
    id Int64 COMMENT 'Unique identifier for each Hacker News item',
    deleted Int64 COMMENT 'Flag indicating if the item has been deleted (1 = deleted, 0 = active)',
    type String COMMENT 'Type of the item (story, comment, job, ask, poll, pollopt)',
    by String COMMENT 'Username of the item author',
    time DateTime COMMENT 'Timestamp when the item was created',
    text String COMMENT 'Content text of the item (for comments, stories, etc.)',
    dead Int64 COMMENT 'Flag indicating if the item is dead/killed (1 = dead, 0 = alive)',
    parent Int64 COMMENT 'ID of the parent item (for comments, this is the story or comment being replied to)',
    poll Int64 COMMENT 'ID of the poll this item belongs to (for poll options)',
    kids Array(String) COMMENT 'Array of IDs of the item children/replies',
    url String COMMENT 'URL associated with the story or item',
    score Int64 COMMENT 'Score/points of the item based on upvotes',
    title String COMMENT 'Title of the story or item',
    parts Array(String) COMMENT 'Array of related poll option IDs (for polls)',
    descendants Int64 COMMENT 'Total number of descendants/replies in the comment tree'
) ENGINE = MergeTree()
ORDER BY (type, id)
COMMENT 'Main table storing all Hacker News items including stories, comments, jobs, and polls';

CREATE TABLE default.hacker_news_subset (
    id Int64 COMMENT 'Unique identifier for each Hacker News item',
    text String COMMENT 'Content text of the item',
    by String COMMENT 'Username of the item author'
) ENGINE = MergeTree()
ORDER BY id
COMMENT 'Subset table containing only essential fields from Hacker News items for simplified queries';

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
