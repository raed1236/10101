CREATE TABLE "dlc_messages" (
    -- We need to store the hash as TEXT as the BIGINT type overflows on some u64 values breaking the hash value.
    message_hash TEXT PRIMARY KEY NOT NULL,
    inbound BOOLEAN NOT NULL,
    peer_id TEXT NOT NULL,
    message_type TEXT NOT NULL,
    message_sub_type TEXT NOT NULL,
    timestamp BIGINT NOT NULL
);

CREATE TABLE "last_outbound_dlc_messages" (
    peer_id TEXT PRIMARY KEY NOT NULL,
    message_hash TEXT REFERENCES dlc_messages(message_hash) NOT NULL,
    message TEXT NOT NULL,
    timestamp BIGINT NOT NULL
);

