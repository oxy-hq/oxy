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
(1, 0, 'story', 'user1', now(), 'Example text 1', 0, 0, 0, [], 'http://example.com/1', 10, 'Example Title 1', [], 0);

INSERT INTO default.hacker_news_subset (id, text, by) VALUES 
(1, 'Example text 1', 'user1');
