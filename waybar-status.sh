#!/bin/bash
# Waybar custom module script for obs-pedal
# Reads state from /tmp/obs-pedal-state.json and outputs waybar JSON

STATE_FILE="/tmp/obs-pedal-state.json"

if [[ ! -f "$STATE_FILE" ]]; then
    echo '{"text": "", "class": "inactive"}'
    exit 0
fi

# Read state
current_scene=$(jq -r '.current_scene' "$STATE_FILE" 2>/dev/null)
recording=$(jq -r '.recording' "$STATE_FILE" 2>/dev/null)
scenes=$(jq -r '.scenes[]' "$STATE_FILE" 2>/dev/null)

if [[ -z "$current_scene" ]]; then
    echo '{"text": "", "class": "inactive"}'
    exit 0
fi

# Build display: highlight current scene, dim others
text=""
while IFS= read -r scene; do
    if [[ "$scene" == "$current_scene" ]]; then
        text+="<span font_weight='bold'>$scene</span>"
    else
        text+="<span alpha='50%'>$scene</span>"
    fi
    text+="  "
done <<< "$scenes"

# Add REC indicator
if [[ "$recording" == "true" ]]; then
    text+="<span color='#ff4444' font_weight='bold'>⏺ REC</span>"
    class="recording"
else
    class="idle"
fi

# Remove trailing spaces
text="${text%  }"

echo "{\"text\": \"$text\", \"class\": \"$class\", \"tooltip\": \"OBS: $current_scene\"}"
