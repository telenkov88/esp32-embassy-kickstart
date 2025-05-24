PASSWORD?='DEMO_WIFI_PASSWORD'
SSID?='DEMO_WIFI_SSID'

deps:
	echo "Installing dependencies"
	cargo install espup
	espup install

clean:
	cargo clean
	rm -rf app.bin

build:
	PASSWORD=${PASSWORD} SSID=${SSID} cargo build

release: clean
	PASSWORD=${PASSWORD} SSID=${SSID} cargo build --release

firmware:
	espflash save-image --chip esp32s3 --partition-table=./partitions.csv -s 16mb target/xtensa-esp32s3-none-elf/release/dual-core app.bin

flash:
	espflash flash --partition-table=./partitions.csv -s 16mb --monitor --chip esp32s3 ./target/xtensa-esp32s3-none-elf/release/dual-core

write-bin: firmware
	espflash write-bin --chip esp32s3 0x10000 app.bin && \
	espflash write-bin --chip esp32s3 0x410000 app.bin

monitor:
	espflash monitor

run:
	PASSWORD=${PASSWORD} SSID=${SSID} cargo run
