-- Dashboard parity: memory cache, disk IO counters, per-core CPU (JSON).
ALTER TABLE metric_snapshots ADD COLUMN memory_cached_bytes INTEGER;
ALTER TABLE metric_snapshots ADD COLUMN disk_read_bytes INTEGER;
ALTER TABLE metric_snapshots ADD COLUMN disk_write_bytes INTEGER;
ALTER TABLE metric_snapshots ADD COLUMN cpu_per_core TEXT;
