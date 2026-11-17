#!/usr/bin/env bash
set -e

echo "🚀 بدء تشغيل بيئة تطوير جامعتي (Jami3ati)..."

if [ -f .env ]; then
  export $(cat .env | grep -v '#' | awk '/=/ {print $1}')
fi

echo "📦 تشغيل الحاويات عبر Docker Compose..."
docker-compose up --build -d

echo "⏳ في انتظار جاهزية قاعدة البيانات..."
docker-compose exec -T db pg_isready -U ${POSTGRES_USER:-jami3ati} -d ${POSTGRES_DB:-jami3ati_db}

echo "🌱 حقن البيانات التجريبية والمحققة..."
docker-compose exec -T backend python app/seed/seed.py

echo "✅ تم التشغيل بنجاح!"
echo "🌐 الواجهة: http://localhost:3000"
echo "🔌 واجهة الـ API والتوثيق التفاعلي: http://localhost:8000/api/v1/docs"
