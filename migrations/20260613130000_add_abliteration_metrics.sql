-- Add abliteration metrics to deliveries.
ALTER TABLE deliveries ADD COLUMN kl_divergence REAL NOT NULL DEFAULT 0.0;
ALTER TABLE deliveries ADD COLUMN refused INTEGER NOT NULL DEFAULT 0;
ALTER TABLE deliveries ADD COLUMN total_prompts INTEGER NOT NULL DEFAULT 0;
