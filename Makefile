# A simple makefile for running the server and web app

## Serve the app

.PHONY: desktop
desktop: services-up
	export $$(grep -v '^#' .env | xargs);\
	cd desktop;\
	dx serve

.PHONY: mobile
mobile: services-up
	export $$(grep -v '^#' .env | xargs);\
	cd mobile;\
	dx serve

.PHONY: web
web: services-up
	export $$(grep -v '^#' .env | xargs);\
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

## Bundle the app

.PHONY: bundle-desktop
bundle-desktop: services-up
	export $$(grep -v '^#' .env | xargs);\
	cd desktop;\
	dx bundle

.PHONY: bundle-mobile
bundle-mobile: services-up
	export $$(grep -v '^#' .env | xargs);\
	cd mobile;\
	dx bundle

.PHONY: bundle-web
bundle-web: services-up
	export $$(grep -v '^#' .env | xargs);\
	cd web;\
	dx bundle
