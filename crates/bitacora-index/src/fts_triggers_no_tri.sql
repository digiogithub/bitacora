-- FTS sync triggers (dropped and recreated around the cold-build fast path).
CREATE TRIGGER blocks_ai AFTER INSERT ON blocks BEGIN
  INSERT INTO blocks_fts(rowid, search_text)     VALUES (new.id, new.search_text);
END;
CREATE TRIGGER blocks_ad AFTER DELETE ON blocks BEGIN
  INSERT INTO blocks_fts(blocks_fts, rowid, search_text)         VALUES ('delete', old.id, old.search_text);
END;
CREATE TRIGGER blocks_au AFTER UPDATE OF search_text ON blocks BEGIN
  INSERT INTO blocks_fts(blocks_fts, rowid, search_text)         VALUES ('delete', old.id, old.search_text);
  INSERT INTO blocks_fts(rowid, search_text)     VALUES (new.id, new.search_text);
END;
CREATE TRIGGER pages_ai AFTER INSERT ON pages BEGIN
  INSERT INTO pages_fts(rowid, search_title) VALUES (new.id, new.search_title);
END;
CREATE TRIGGER pages_ad AFTER DELETE ON pages BEGIN
  INSERT INTO pages_fts(pages_fts, rowid, search_title) VALUES ('delete', old.id, old.search_title);
END;
CREATE TRIGGER pages_au AFTER UPDATE OF search_title ON pages BEGIN
  INSERT INTO pages_fts(pages_fts, rowid, search_title) VALUES ('delete', old.id, old.search_title);
  INSERT INTO pages_fts(rowid, search_title) VALUES (new.id, new.search_title);
END;

