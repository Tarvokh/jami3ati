#!/usr/bin/env bash
set -e

echo "🌱 جاري إعادة حقن بيانات العرض التجريبية لمشروع جامعتي..."
docker-compose exec -T backend python app/seed/seed.py
echo "✅ اكتمل حقن البيانات التجريبية والمحققة."
