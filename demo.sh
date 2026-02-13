#!/bin/bash
# Axiom Full Loop Demo
# Demonstrates the AI feedback loop: generate → validate → simulate → measure feel → tune
#
# Prerequisites: axiom must be running (cargo run) with API on localhost:3000

API="http://127.0.0.1:3000"

echo "========================================="
echo "  Axiom: AI-Native Game Engine Demo"
echo "========================================="
echo ""

# Step 1: Check API is running
echo "[1/7] Checking API..."
STATE=$(curl -s "$API/state")
if [ $? -ne 0 ]; then
    echo "ERROR: Axiom is not running. Start it with: run.bat"
    exit 1
fi
echo "  API is live."
echo ""

# Step 2: Get current physics
echo "[2/7] Current physics config:"
curl -s "$API/physics" | python -m json.tool 2>/dev/null || curl -s "$API/physics"
echo ""

# Step 3: Generate 3 levels of increasing difficulty
echo "[3/7] Generating levels..."
for DIFF in 0.2 0.5 0.8; do
    echo "  Generating level (difficulty=$DIFF)..."
    RESULT=$(curl -s -X POST "$API/generate" \
        -H "Content-Type: application/json" \
        -d "{\"difficulty\": $DIFF, \"seed\": 42, \"constraints\": [\"reachable\", \"bounds_check\"]}")

    VALID=$(echo "$RESULT" | python -c "import sys,json; d=json.load(sys.stdin); print(d.get('data',{}).get('validation',{}).get('valid','?'))" 2>/dev/null || echo "?")
    JUMPS=$(echo "$RESULT" | python -c "import sys,json; d=json.load(sys.stdin); print(d.get('data',{}).get('difficulty_metrics',{}).get('required_jumps','?'))" 2>/dev/null || echo "?")
    echo "    Valid: $VALID | Required jumps: $JUMPS"
done
echo ""

# Step 4: Generate a medium difficulty level and load it
echo "[4/7] Loading medium difficulty level..."
LEVEL=$(curl -s -X POST "$API/generate" \
    -H "Content-Type: application/json" \
    -d '{"difficulty": 0.4, "seed": 123, "width": 60, "height": 20, "constraints": ["reachable", "bounds_check"]}')

# Extract tilemap and load it
WIDTH=$(echo "$LEVEL" | python -c "import sys,json; print(json.load(sys.stdin)['data']['tilemap']['width'])" 2>/dev/null)
HEIGHT=$(echo "$LEVEL" | python -c "import sys,json; print(json.load(sys.stdin)['data']['tilemap']['height'])" 2>/dev/null)
TILES=$(echo "$LEVEL" | python -c "import sys,json; print(json.dumps(json.load(sys.stdin)['data']['tilemap']['tiles']))" 2>/dev/null)
SPAWN=$(echo "$LEVEL" | python -c "import sys,json; d=json.load(sys.stdin)['data']; print(json.dumps(d['player_spawn']))" 2>/dev/null)
GOAL=$(echo "$LEVEL" | python -c "import sys,json; d=json.load(sys.stdin)['data']; print(json.dumps(d['goal']))" 2>/dev/null)

curl -s -X POST "$API/level" \
    -H "Content-Type: application/json" \
    -d "{\"width\": $WIDTH, \"height\": $HEIGHT, \"tiles\": $TILES, \"player_spawn\": $SPAWN, \"goal\": $GOAL}" > /dev/null

echo "  Level loaded (${WIDTH}x${HEIGHT})"
echo ""

# Step 5: Simulate a bot completing the level
echo "[5/7] Simulating bot playthrough..."
SIM_RESULT=$(curl -s -X POST "$API/simulate" \
    -H "Content-Type: application/json" \
    -d '{
        "inputs": [
            {"frame": 0, "action": "right", "duration": 600},
            {"frame": 30, "action": "jump", "duration": 1},
            {"frame": 90, "action": "jump", "duration": 1},
            {"frame": 150, "action": "jump", "duration": 1},
            {"frame": 210, "action": "jump", "duration": 1},
            {"frame": 270, "action": "jump", "duration": 1},
            {"frame": 330, "action": "jump", "duration": 1},
            {"frame": 390, "action": "jump", "duration": 1},
            {"frame": 450, "action": "jump", "duration": 1}
        ],
        "max_frames": 600,
        "record_interval": 60
    }')

OUTCOME=$(echo "$SIM_RESULT" | python -c "import sys,json; print(json.load(sys.stdin).get('data',{}).get('outcome','?'))" 2>/dev/null || echo "?")
FRAMES=$(echo "$SIM_RESULT" | python -c "import sys,json; print(json.load(sys.stdin).get('data',{}).get('frames_elapsed','?'))" 2>/dev/null || echo "?")
EVENTS=$(echo "$SIM_RESULT" | python -c "import sys,json; print(len(json.load(sys.stdin).get('data',{}).get('events',[])))" 2>/dev/null || echo "?")
echo "  Outcome: $OUTCOME"
echo "  Frames: $FRAMES"
echo "  Events: $EVENTS"
echo ""

# Step 6: Measure jump feel
echo "[6/7] Measuring jump feel..."
FEEL=$(curl -s "$API/feel/jump")
echo "  Current jump profile:"
echo "$FEEL" | python -m json.tool 2>/dev/null || echo "$FEEL"
echo ""

# Compare to Celeste
echo "  Comparing to Celeste reference..."
COMPARE=$(curl -s "$API/feel/compare?target=celeste")
MATCH=$(echo "$COMPARE" | python -c "import sys,json; print('{:.1f}%'.format(json.load(sys.stdin).get('data',{}).get('overall_match_pct',0)))" 2>/dev/null || echo "?")
echo "  Match: $MATCH"
echo ""

# Step 7: Auto-tune to match Celeste
echo "[7/7] Auto-tuning physics to match Celeste..."
TUNE=$(curl -s -X POST "$API/feel/tune" \
    -H "Content-Type: application/json" \
    -d '{"target": "celeste"}')
TUNE_MATCH=$(echo "$TUNE" | python -c "import sys,json; print('{:.1f}%'.format(json.load(sys.stdin).get('data',{}).get('match_pct',0)))" 2>/dev/null || echo "?")
echo "  After tuning, match: $TUNE_MATCH"
echo ""

echo "========================================="
echo "  Demo complete!"
echo "  The game is running in windowed mode."
echo "  Try playing with WASD/Space."
echo "  API available at $API"
echo "========================================="
