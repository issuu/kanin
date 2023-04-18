default:
	just --list

dev:
	cargo watch --clear --exec clippy --exec test

test:
	docker compose up --renew-anon-volumes --detach
	cargo test || (docker compose down && false)
	docker compose down
