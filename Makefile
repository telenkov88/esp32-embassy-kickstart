PASSWORD?='MyDefaultPsw'
SSID?='MyDefaultSSID'

DOCKER_IMG = ghcr.io/telenkov88/idf-rust-esp32:latest


DOCKER_ARGS = -it --rm \
              --mount type=bind,src=$(shell pwd)/src,dst=/app/src,ro \
              --mount type=bind,src=$(shell pwd)/Makefile,dst=/app/Makefile,ro \
              --mount type=bind,src=$(shell pwd)/build.rs,dst=/app/build.rs,ro \
              --mount type=bind,src=$(shell pwd)/.cargo,dst=/app/.cargo,ro \
              --mount type=bind,src=$(shell pwd)/Cargo.toml,dst=/app/Cargo.toml,ro \
              --mount type=bind,src=$(shell pwd)/Cargo.lock,dst=/app/Cargo.lock,ro \
              --mount type=bind,src=$(shell pwd)/partitions.csv,dst=/app/partitions.csv,ro \
              --mount type=bind,src=$(shell pwd)/output,dst=/app/output


ESPFLASH_ARGS = --chip esp32s3 \
              --partition-table=./partitions.csv \
              -s 16mb \
              target/xtensa-esp32s3-none-elf/release/dual-core

deps:
	echo "Installing dependencies"
	cargo install espup
	rustup default esp
	espup install
	. $HOME/export-esp.sh

clean:
	cargo clean
	rm -rf output/firmware.bin

build:
	PASSWORD=${PASSWORD} SSID=${SSID} cargo build

docker:
	docker buildx build -f dockerfiles/Dockerfile --progress=plain --load -t ${DOCKER_IMG} .

docker-build:
	mkdir -p -m 777 output
	rm -rf output/firmware.bin
	docker run ${DOCKER_ARGS} ${DOCKER_IMG} bash -c 'make release && make firmware'

release: clean
	PASSWORD=${PASSWORD} SSID=${SSID} cargo build --release

stats:
	xtensa-esp32-elf-size -A target/xtensa-esp32s3-none-elf/release/dual-core

dram-usage:
	cargo bloat --release --crates
	cargo bloat --release --functions

firmware:
	mkdir -p output
	espflash save-image ${ESPFLASH_ARGS} output/firmware.bin
	espflash partition-table --to-binary partitions.csv > output/partitions.bin
	cp -r partitions.csv output/partitions.csv
	chmod 777 output/partitions.csv
	chmod 777 output/firmware.bin
	chmod 777 output/partitions.bin

erase:
	espflash erase-flash

flash:
	espflash flash ${ESPFLASH_ARGS} -B 921600

flash-firmware: firmware
	espflash write-bin --chip esp32s3 0x8000 output/partitions.bin
	espflash write-bin --chip esp32s3 0x10000 output/firmware.bin
	espflash write-bin --chip esp32s3 0x510000 output/firmware.bin

monitor:
	espflash monitor

run:
	PASSWORD=${PASSWORD} SSID=${SSID} cargo run
