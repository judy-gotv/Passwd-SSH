-- Usage counter for the Frequent view (Xterminal-style).
ALTER TABLE hosts ADD COLUMN connect_count INTEGER NOT NULL DEFAULT 0;
