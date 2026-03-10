#!/bin/bash

# Grafana Dashboard Backup and Restore Script
# Downloads/uploads Grafana dashboard JSON files

# Initially generated at https://claude.ai/share/966a1ce8-12f7-4260-b069-c2f706db5d26
# But has been modified since

set -e

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Display usage information
usage() {
    cat << EOF
Usage: $0 [OPTIONS]

Options:
    -d, --download DIR      Download dashboards to specified directory (example: ./grafana/dashboards)
    -u, --upload DIR        Upload dashboards from specified directory (example: ./grafana/dashboards)
    -h, --host URL          Grafana host URL (default: https://nousresearch.grafana.net)
    -f, --folder UID        Folder UID to filter downloads (default: cepkagw8c5gqoe)
    --help                  Display this help message

Environment Variables:
    GRAFANA_TOKEN          Required. Grafana API token with dashboard permissions

Examples:
    # Download dashboards from default folder
    export GRAFANA_TOKEN="your-token-here"
    $0 --download ./grafana-backups --host https://myorg.grafana.net

    # Download dashboards from specific folder
    $0 --download ./grafana-backups --folder abc123def --host https://myorg.grafana.net

    # Upload dashboards
    $0 --upload ./grafana-backups --host https://myorg.grafana.net
EOF
    exit 0
}

# Check if authentication token is set
check_token() {
    if [ -z "$GRAFANA_TOKEN" ]; then
        echo -e "${RED}Error: GRAFANA_TOKEN environment variable is not set${NC}"
        echo "Please set your Grafana API token:"
        echo "  export GRAFANA_TOKEN='your-token-here'"
        exit 1
    fi
}

# Download dashboards from Grafana
download_dashboards() {
    local dir="$1"
    local host="$2"
    local folder_uid="$3"

    echo -e "${GREEN}Downloading dashboards from $host${NC}"
    echo "Filtering by folder UID: $folder_uid"

    # Create directory if it doesn't exist
    mkdir -p "$dir"

    # Get list of all dashboards in specified folder
    echo "Fetching dashboard list..."
    local search_response=$(curl -s -H "Authorization: Bearer $GRAFANA_TOKEN" \
        "$host/api/search?type=dash-db&folderUIDs=$folder_uid")

    if [ $? -ne 0 ]; then
        echo -e "${RED}Error: Failed to connect to Grafana${NC}"
        exit 1
    fi

    # Check if response is valid JSON
    if ! echo "$search_response" | jq empty 2>/dev/null; then
        echo -e "${RED}Error: Invalid response from Grafana API${NC}"
        echo "Response: $search_response"
        exit 1
    fi

    local dashboard_count=$(echo "$search_response" | jq '. | length')
    echo "Found $dashboard_count dashboards"

    # Download each dashboard
    local success_count=0
    echo "$search_response" | jq -c '.[]' | while read -r dashboard; do
        local uid=$(echo "$dashboard" | jq -r '.uid')
        local title=$(echo "$dashboard" | jq -r '.title')
        local folder=$(echo "$dashboard" | jq -r '.folderTitle // "General"')

        # Sanitize filename
        local safe_title=$(echo "$title" | sed 's/[^a-zA-Z0-9._-]/_/g')
        local safe_folder=$(echo "$folder" | sed 's/[^a-zA-Z0-9._-]/_/g')

        # Create folder structure
        local target_dir="$dir/$safe_folder"
        mkdir -p "$target_dir"

        local filename="$target_dir/${safe_title}_${uid}.json"

        echo "  Downloading: $title ($uid)"

        # Get dashboard JSON
        local dashboard_json=$(curl -s -H "Authorization: Bearer $GRAFANA_TOKEN" \
            "$host/api/dashboards/uid/$uid")

        if echo "$dashboard_json" | jq -e '.dashboard' > /dev/null 2>&1; then
            echo "$dashboard_json" | jq '.dashboard' > "$filename"
            echo -e "    ${GREEN}✓${NC} Saved to: $filename"
        else
            echo -e "    ${RED}✗${NC} Failed to download"
        fi
    done

    echo -e "${GREEN}Download complete!${NC}"
    echo "Dashboards saved to: $dir"
}

# Upload dashboards to Grafana
upload_dashboards() {
    local dir="$1"
    local host="$2"
    local folder_uid="$3"

    echo -e "${GREEN}Uploading dashboards to $host${NC}"

    if [ ! -d "$dir" ]; then
        echo -e "${RED}Error: Directory $dir does not exist${NC}"
        exit 1
    fi

    # Find all JSON files
    local json_files=$(find "$dir" -type f -name "*.json")

    if [ -z "$json_files" ]; then
        echo -e "${YELLOW}Warning: No JSON files found in $dir${NC}"
        exit 0
    fi

    local success_count=0
    local fail_count=0

    while IFS= read -r file; do
        echo "  Uploading: $(basename "$file")"

        # Extract dashboard JSON and prepare payload
        local dashboard=$(cat "$file" | jq '.')

        if [ "$dashboard" = "null" ]; then
            echo -e "    ${YELLOW}⚠${NC} Invalid format, skipping"
            ((fail_count++))
            continue
        fi

        # Create upload payload
        local payload=$(jq -n \
                --argjson dashboard "$dashboard" \
            "{dashboard: \$dashboard, overwrite: true, folderUid: \"$folder_uid\"}")

        # Upload dashboard
        local response=$(curl -s -X POST \
                -H "Authorization: Bearer $GRAFANA_TOKEN" \
                -H "Content-Type: application/json" \
                -d "$payload" \
            "$host/api/dashboards/db")

        if echo "$response" | jq -e '.status == "success"' > /dev/null 2>&1; then
            echo -e "    ${GREEN}✓${NC} Uploaded successfully"
            ((success_count++))
        else
            echo -e "    ${RED}✗${NC} Upload failed"
            echo "$response" | jq -r '.message // "Unknown error"' | sed 's/^/      /'
            ((fail_count++))
        fi
    done <<< "$json_files"

    echo ""
    echo -e "${GREEN}Upload complete!${NC}"
    echo "Success: $success_count | Failed: $fail_count"
}

# Main script logic
main() {
    local mode=""
    local directory=""
    local host="https://nousresearch.grafana.net"
    local folder_uid="cepkagw8c5gqoe"

    # Parse command line arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            -d|--download)
                mode="download"
                directory="$2"
                shift 2
                ;;
            -u|--upload)
                mode="upload"
                directory="$2"
                shift 2
                ;;
            -h|--host)
                host="$2"
                shift 2
                ;;
            -f|--folder)
                folder_uid="$2"
                shift 2
                ;;
            --help)
                usage
                ;;
            *)
                echo -e "${RED}Error: Unknown option $1${NC}"
                echo "Use --help for usage information"
                exit 1
                ;;
        esac
    done

    # Validate arguments
    if [ -z "$mode" ]; then
        echo -e "${RED}Error: Must specify either --download or --upload${NC}"
        echo "Use --help for usage information"
        exit 1
    fi

    if [ -z "$directory" ]; then
        echo -e "${RED}Error: Directory not specified${NC}"
        echo "Use --help for usage information"
        exit 1
    fi

    # Check for required token
    check_token

    # Check for jq dependency
    if ! command -v jq &> /dev/null; then
        echo -e "${RED}Error: jq is required but not installed${NC}"
        echo "Please install jq: https://stedolan.github.io/jq/download/"
        exit 1
    fi

    # Remove trailing slash from host
    host="${host%/}"

    # Execute requested operation
    case $mode in
        download)
            download_dashboards "$directory" "$host" "$folder_uid"
            ;;
        upload)
            upload_dashboards "$directory" "$host" "$folder_uid"
            ;;
    esac
}

# Run main function
main "$@"