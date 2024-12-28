# A simple makefile for running the server and web app

.PHONY: desktop
desktop: services-up
	source .env;\
	cd desktop;\
	dx serve

.PHONY: mobile
mobile: services-up
	source .env;\
	cd mobile;\
	dx serve

.PHONY: web
web: services-up
	source .env;\
	cd web;\
	dx serve

.PHONY: services-up
services-up:
	cd server/services/compose;\
	docker compose --env-file ../../../.env up -d;\
	echo "Waiting for PostgreSQL to start...";\
	until docker exec typednotes_db pg_isready -U user; do\
		sleep 1;\
	done;

.PHONY: services-restart
services-restart:
	cd server/services/compose;\
	docker compose restart;

.PHONY: services-down
services-down:
	cd server/services/compose;\
	docker compose down;

.PHONY: update
update:
	cargo update