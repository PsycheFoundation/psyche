#!/bin/bash
sed -i 's/server nonexistent-hostname.invalid:8899;/server psyche-solana-test-validator:8899;/' /tmp/peter/psyche/docker/test/subscriptions_test/nginx_http.conf
docker exec nginx-http-proxy nginx -s reload
echo "DNS RESTORED - nginx now pointing to correct host"
