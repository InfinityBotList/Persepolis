CDN_PATH := /silverpelt/cdn/ibl

all:
	cargo build --release
restartwebserver:
	cargo sqlx prepare
	make all
	make restartwebserver_nobuild

restartwebserver_nobuild:
	sudo systemctl stop persepolis
	sleep 3 # Give time for it to stop
	cp -v target/release/persepolis persepolis
	sudo systemctl start persepolis

ts:
	rm -rvf $(CDN_PATH)/dev/bindings/persepolis
	cargo test
	cp -rf .generated $(CDN_PATH)/dev/bindings/persepolis
