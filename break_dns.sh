#!/bin/bash
sed -i 's/server psyche-solana-test-validator:8899;/server nonexistent-hostname.invalid:8899;/' /tmp/peter/psyche/docker/test/subscriptions_test/nginx_http.conf
docker exec nginx-http-proxy nginx -s reload
echo "DNS BROKEN - nginx now pointing to nonexistent host"
