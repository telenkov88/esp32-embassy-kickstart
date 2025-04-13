PASSWORD?='DEMO_WIFI_PASSWORD'
SSID?='DEMO_WIFI_SSID'

deps:
	echo "Installing dependencies"
	cargo install espup
	espup install


build:
	PASSWORD=${PASSWORD} SSID=${SSID} cargo build

run:
	PASSWORD=${PASSWORD} SSID=${SSID} cargo run
