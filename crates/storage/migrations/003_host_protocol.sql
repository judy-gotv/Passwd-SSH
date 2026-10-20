-- Connection protocol: ssh (default) / telnet / vnc / rdp.
ALTER TABLE hosts ADD COLUMN protocol TEXT NOT NULL DEFAULT 'ssh';
