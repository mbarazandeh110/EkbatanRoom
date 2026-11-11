---
name: distributed-room-placement
description: Distributed storage pattern for room topology across SFU nodes using Redis, local in-memory caching, and atomic locking to prevent split-brain scenarios.
---

# Specialized Skill: Distributed Room Placement Registry (`RoomPlacementRegistry`)

When managing the distribution of a room across one or more `sfu-media-node` instances, adhere to the following rules:

## 1. Redis Key Schema
1. **Atomic Room Creation Lock (`rtc:room:lock:{room_id}`):**
   - Acquired via `SET NX PX 5000` prior to invoking `CreateRouter`, and released atomically using a Lua script (`if redis.call("get",KEYS[1]) == ARGV[1] then return redis.call("del",KEYS[1]) else return 0 end`).
2. **Base Room Placement Metadata (`rtc:room:placement:{room_id}` - Hash):**
   - Fields: `room_id`, `primary_node_id`, `primary_router_id`, `primary_worker_idx`, `created_at`, `updated_at`.
3. **Active Room Routers in Multi-Worker and Multi-Node Topologies (`rtc:room:routers:{room_id}` - Hash):**
   - Field Key: `{node_id}:{router_id}`
   - Field Value (JSON): Contains `node_id`, `router_id`, `worker_index`, `role` (`primary` | `intra_node_pipe` | `inter_node_pipe`), and `active_consumers`.

## 2. Read-Through Cache Pattern with Pub/Sub Synchronization
- To prevent network round-trips to Redis on every signaling packet, `LocalityScheduler` caches the topology of active rooms in local memory (`sync.RWMutex` / `RoomActor`).
- Whenever a new spillover router (`PipeTransport`) is added to a room or a room is closed, Redis is updated first, followed by publishing a lightweight event to the `rtc:events:topology` channel so other `signaling-coordinator` pods can invalidate or update their local caches.
