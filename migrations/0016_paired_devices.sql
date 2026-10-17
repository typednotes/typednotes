-- Broker device keys stay in the vault; this table contains only delivery
-- cursors for separately authorized conversation selectors. Retry-safe inbox
-- insertion precedes advancing the corresponding cursor.
CREATE TABLE paired_inbox_cursors (
    channel_id uuid NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    selector text NOT NULL,
    cursor numeric(20, 0) NOT NULL DEFAULT 0 CHECK (cursor >= 0 AND cursor < 18446744073709551616),
    PRIMARY KEY (channel_id, selector)
);
