package placement_registry

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"strconv"
	"sync"
	"time"

	"github.com/google/uuid"
	"github.com/redis/go-redis/v9"

	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

var _ ports.RoomPlacementRegistryPort = (*RedisPlacementRegistry)(nil)

const (
	// TopologyEventsChannel is the Redis Pub/Sub channel for cross-pod cache synchronization.
	TopologyEventsChannel = "rtc:events:topology"
	// DefaultPlacementTTL is the default key expiration for room placement hashes.
	DefaultPlacementTTL = 24 * time.Hour
)

var releaseLockScript = redis.NewScript(`
if redis.call("get", KEYS[1]) == ARGV[1] then
    return redis.call("del", KEYS[1])
else
    return 0
end
`)

// TopologyEventPayload represents a lightweight PubSub event to synchronize caches across pods.
type TopologyEventPayload struct {
	Action string `json:"action"` // "update" or "remove"
	RoomID string `json:"room_id"`
}

// RedisPlacementRegistry provides a production-grade distributed implementation of RoomPlacementRegistryPort
// backed by Redis and featuring atomic locking, hash storage, and a read-through cache with PubSub invalidation.
type RedisPlacementRegistry struct {
	client redis.UniversalClient
	ttl    time.Duration

	cacheMu sync.RWMutex
	cache   map[string]*domain.RoomPlacementTopology

	subCancel context.CancelFunc
	pubsub    *redis.PubSub
}

// NewRedisPlacementRegistry constructs a Redis-backed placement registry.
func NewRedisPlacementRegistry(client redis.UniversalClient, ttl ...time.Duration) *RedisPlacementRegistry {
	defaultTTL := DefaultPlacementTTL
	if len(ttl) > 0 && ttl[0] > 0 {
		defaultTTL = ttl[0]
	}

	return &RedisPlacementRegistry{
		client: client,
		ttl:    defaultTTL,
		cache:  make(map[string]*domain.RoomPlacementTopology),
	}
}

// StartPubSubSubscriber initiates a background goroutine listening to rtc:events:topology for cache sync.
func (r *RedisPlacementRegistry) StartPubSubSubscriber(ctx context.Context) {
	subCtx, cancel := context.WithCancel(ctx)
	r.subCancel = cancel
	r.pubsub = r.client.Subscribe(subCtx, TopologyEventsChannel)

	go func() {
		ch := r.pubsub.Channel()
		for {
			select {
			case <-subCtx.Done():
				return
			case msg, ok := <-ch:
				if !ok {
					return
				}
				var payload TopologyEventPayload
				if err := json.Unmarshal([]byte(msg.Payload), &payload); err == nil && payload.RoomID != "" {
					if payload.Action == "remove" {
						r.deleteFromLocalCache(payload.RoomID)
					} else {
						r.invalidateLocalCache(payload.RoomID)
					}
				}
			}
		}
	}()
}

// Close gracefully stops the subscriber and closes pubsub resources.
func (r *RedisPlacementRegistry) Close() error {
	if r.subCancel != nil {
		r.subCancel()
	}
	if r.pubsub != nil {
		return r.pubsub.Close()
	}
	return nil
}

// AcquireRoomCreationLock acquires an atomic distributed lock (SET NX PX) to prevent split-brain room placement.
func (r *RedisPlacementRegistry) AcquireRoomCreationLock(ctx context.Context, roomID string, ttl time.Duration) (func(), error) {
	lockKey := fmt.Sprintf("rtc:room:lock:{%s}", roomID)
	token := uuid.NewString()

	ok, err := r.client.SetNX(ctx, lockKey, token, ttl).Result()
	if err != nil {
		return nil, fmt.Errorf("failed to acquire redis room lock: %w", err)
	}
	if !ok {
		return nil, domain.ErrRoomLockAcquisitionFailed
	}

	var once sync.Once
	unlockFunc := func() {
		once.Do(func() {
			bgCtx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
			defer cancel()
			_ = releaseLockScript.Run(bgCtx, r.client, []string{lockKey}, token).Err()
		})
	}

	return unlockFunc, nil
}

// GetRoomPlacement retrieves the room placement topology, checking local cache first before Redis.
func (r *RedisPlacementRegistry) GetRoomPlacement(ctx context.Context, roomID string) (*domain.RoomPlacementTopology, error) {
	// 1. Read-Through in-memory cache lookup
	r.cacheMu.RLock()
	cached, exists := r.cache[roomID]
	r.cacheMu.RUnlock()
	if exists && cached != nil {
		return copyTopology(cached), nil
	}

	// 2. Fetch from Redis Hash rtc:room:placement:{room_id}
	placementKey := fmt.Sprintf("rtc:room:placement:{%s}", roomID)
	routersKey := fmt.Sprintf("rtc:room:routers:{%s}", roomID)

	meta, err := r.client.HGetAll(ctx, placementKey).Result()
	if err != nil {
		return nil, fmt.Errorf("failed to read room placement hash: %w", err)
	}
	if len(meta) == 0 {
		return nil, domain.ErrRoomPlacementNotFound
	}

	workerIdx, _ := strconv.ParseUint(meta["primary_worker_idx"], 10, 32)
	createdAt, _ := time.Parse(time.RFC3339Nano, meta["created_at"])
	updatedAt, _ := time.Parse(time.RFC3339Nano, meta["updated_at"])

	topo := &domain.RoomPlacementTopology{
		RoomID:           meta["room_id"],
		PrimaryNodeID:    meta["primary_node_id"],
		PrimaryRouterID:  meta["primary_router_id"],
		PrimaryWorkerIdx: uint32(workerIdx),
		CreatedAt:        createdAt,
		UpdatedAt:        updatedAt,
		Routers:          make([]domain.RouterPlacementEntry, 0),
	}

	// 3. Fetch all active routers from rtc:room:routers:{room_id}
	routersMap, err := r.client.HGetAll(ctx, routersKey).Result()
	if err == nil && len(routersMap) > 0 {
		for _, rawJSON := range routersMap {
			var entry domain.RouterPlacementEntry
			if err := json.Unmarshal([]byte(rawJSON), &entry); err == nil {
				topo.Routers = append(topo.Routers, entry)
			}
		}
	}

	// Store in local cache
	r.updateLocalCache(topo)

	return copyTopology(topo), nil
}

// SavePrimaryPlacement saves primary placement metadata and registers the primary router.
func (r *RedisPlacementRegistry) SavePrimaryPlacement(ctx context.Context, roomID, primaryNodeID, primaryRouterID string, workerIdx uint32) error {
	placementKey := fmt.Sprintf("rtc:room:placement:{%s}", roomID)
	routersKey := fmt.Sprintf("rtc:room:routers:{%s}", roomID)

	now := time.Now().UTC()
	nowStr := now.Format(time.RFC3339Nano)

	primaryEntry := domain.RouterPlacementEntry{
		NodeID:          primaryNodeID,
		RouterID:        primaryRouterID,
		WorkerIndex:     workerIdx,
		Role:            domain.RouterRolePrimary,
		ActiveConsumers: 0,
	}
	entryJSON, err := json.Marshal(primaryEntry)
	if err != nil {
		return fmt.Errorf("failed to marshal primary router entry: %w", err)
	}

	pipe := r.client.TxPipeline()
	pipe.HSet(ctx, placementKey, map[string]interface{}{
		"room_id":            roomID,
		"primary_node_id":    primaryNodeID,
		"primary_router_id":  primaryRouterID,
		"primary_worker_idx": workerIdx,
		"created_at":         nowStr,
		"updated_at":         nowStr,
	})
	pipe.Expire(ctx, placementKey, r.ttl)

	routerField := fmt.Sprintf("%s:%s", primaryNodeID, primaryRouterID)
	pipe.HSet(ctx, routersKey, routerField, string(entryJSON))
	pipe.Expire(ctx, routersKey, r.ttl)

	if _, err := pipe.Exec(ctx); err != nil {
		return fmt.Errorf("failed to save primary placement to redis: %w", err)
	}

	// Update local cache & broadcast topology update
	topo := &domain.RoomPlacementTopology{
		RoomID:           roomID,
		PrimaryNodeID:    primaryNodeID,
		PrimaryRouterID:  primaryRouterID,
		PrimaryWorkerIdx: workerIdx,
		Routers:          []domain.RouterPlacementEntry{primaryEntry},
		CreatedAt:        now,
		UpdatedAt:        now,
	}
	r.updateLocalCache(topo)
	r.publishTopologyEvent(ctx, "update", roomID)

	return nil
}

// AddCascadedRouter registers a secondary or cascaded router in the room's router topology.
func (r *RedisPlacementRegistry) AddCascadedRouter(ctx context.Context, roomID, nodeID string, workerIdx uint32, routerID, pipeRole string) error {
	placementKey := fmt.Sprintf("rtc:room:placement:{%s}", roomID)
	routersKey := fmt.Sprintf("rtc:room:routers:{%s}", roomID)

	exists, err := r.client.Exists(ctx, placementKey).Result()
	if err != nil {
		return fmt.Errorf("failed to check room placement: %w", err)
	}
	if exists == 0 {
		return domain.ErrRoomPlacementNotFound
	}

	entry := domain.RouterPlacementEntry{
		NodeID:          nodeID,
		RouterID:        routerID,
		WorkerIndex:     workerIdx,
		Role:            pipeRole,
		ActiveConsumers: 0,
	}
	entryJSON, err := json.Marshal(entry)
	if err != nil {
		return fmt.Errorf("failed to marshal cascaded router entry: %w", err)
	}

	nowStr := time.Now().UTC().Format(time.RFC3339Nano)
	routerField := fmt.Sprintf("%s:%s", nodeID, routerID)

	pipe := r.client.TxPipeline()
	pipe.HSet(ctx, routersKey, routerField, string(entryJSON))
	pipe.HSet(ctx, placementKey, "updated_at", nowStr)
	pipe.Expire(ctx, routersKey, r.ttl)
	pipe.Expire(ctx, placementKey, r.ttl)

	if _, err := pipe.Exec(ctx); err != nil {
		return fmt.Errorf("failed to add cascaded router to redis: %w", err)
	}

	r.invalidateLocalCache(roomID)
	r.publishTopologyEvent(ctx, "update", roomID)

	return nil
}

// UpdateRouterConsumerCount updates the active consumer count on a specific router.
func (r *RedisPlacementRegistry) UpdateRouterConsumerCount(ctx context.Context, roomID, nodeID, routerID string, activeConsumers uint32) error {
	routersKey := fmt.Sprintf("rtc:room:routers:{%s}", roomID)
	placementKey := fmt.Sprintf("rtc:room:placement:{%s}", roomID)
	routerField := fmt.Sprintf("%s:%s", nodeID, routerID)

	raw, err := r.client.HGet(ctx, routersKey, routerField).Result()
	if err != nil {
		if errors.Is(err, redis.Nil) {
			return domain.ErrRoomPlacementNotFound
		}
		return fmt.Errorf("failed to get router entry from redis: %w", err)
	}

	var entry domain.RouterPlacementEntry
	if err := json.Unmarshal([]byte(raw), &entry); err != nil {
		return fmt.Errorf("failed to unmarshal router entry: %w", err)
	}

	entry.ActiveConsumers = activeConsumers
	entryJSON, err := json.Marshal(entry)
	if err != nil {
		return fmt.Errorf("failed to marshal router entry: %w", err)
	}

	nowStr := time.Now().UTC().Format(time.RFC3339Nano)
	pipe := r.client.TxPipeline()
	pipe.HSet(ctx, routersKey, routerField, string(entryJSON))
	pipe.HSet(ctx, placementKey, "updated_at", nowStr)
	if _, err := pipe.Exec(ctx); err != nil {
		return fmt.Errorf("failed to update router consumers in redis: %w", err)
	}

	r.invalidateLocalCache(roomID)
	r.publishTopologyEvent(ctx, "update", roomID)
	return nil
}

// RemoveRoomPlacement clears placement hashes, router records, and locks from Redis.
func (r *RedisPlacementRegistry) RemoveRoomPlacement(ctx context.Context, roomID string) error {
	placementKey := fmt.Sprintf("rtc:room:placement:{%s}", roomID)
	routersKey := fmt.Sprintf("rtc:room:routers:{%s}", roomID)
	lockKey := fmt.Sprintf("rtc:room:lock:{%s}", roomID)

	_ = r.client.Del(ctx, placementKey, routersKey, lockKey).Err()

	r.deleteFromLocalCache(roomID)
	r.publishTopologyEvent(ctx, "remove", roomID)
	return nil
}

func (r *RedisPlacementRegistry) updateLocalCache(topo *domain.RoomPlacementTopology) {
	r.cacheMu.Lock()
	r.cache[topo.RoomID] = copyTopology(topo)
	r.cacheMu.Unlock()
}

func (r *RedisPlacementRegistry) invalidateLocalCache(roomID string) {
	r.cacheMu.Lock()
	delete(r.cache, roomID)
	r.cacheMu.Unlock()
}

func (r *RedisPlacementRegistry) deleteFromLocalCache(roomID string) {
	r.cacheMu.Lock()
	delete(r.cache, roomID)
	r.cacheMu.Unlock()
}

func (r *RedisPlacementRegistry) publishTopologyEvent(ctx context.Context, action, roomID string) {
	payload, err := json.Marshal(TopologyEventPayload{
		Action: action,
		RoomID: roomID,
	})
	if err != nil {
		return
	}

	if err := r.client.Publish(ctx, TopologyEventsChannel, string(payload)).Err(); err != nil {
		slog.Warn("failed to publish topology update event", "room_id", roomID, "err", err)
	}
}
