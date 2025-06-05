use esp_hal::gpio::OutputPin;
use esp_hal::peripheral::Peripheral;
use esp_hal::rmt::{TxChannel, TxChannelCreator};
use esp_hal_smartled::SmartLedsAdapter;
use smart_leds::RGB8;
use smart_leds::hsv::hsv2rgb;
use smart_leds::{SmartLedsWrite, brightness, gamma, hsv::Hsv};

pub trait NeoPixelChannelCreator<'d, ChannelType: TxChannel, Pin: OutputPin>:
    TxChannelCreator<'d, ChannelType, Pin>
{
}

impl<'d, ChannelCreator, ChannelType, Pin> NeoPixelChannelCreator<'d, ChannelType, Pin>
    for ChannelCreator
where
    ChannelCreator: TxChannelCreator<'d, ChannelType, Pin>,
    ChannelType: TxChannel,
    Pin: OutputPin,
{
}

pub struct NeoPixel<ChannelType: TxChannel, const BUFFER_SIZE: usize> {
    led: SmartLedsAdapter<ChannelType, BUFFER_SIZE>,
    color: Hsv,
    brightness_level: u8,
}

impl<ChannelType: TxChannel, const BUFFER_SIZE: usize> NeoPixel<ChannelType, BUFFER_SIZE> {
    pub fn new<'d, ChannelCreator, Pin>(
        channel: ChannelCreator,
        pin: impl Peripheral<P = Pin> + 'd,
        rmt_buffer: [u32; BUFFER_SIZE],
    ) -> Self
    where
        ChannelCreator: NeoPixelChannelCreator<'d, ChannelType, Pin>,
        Pin: OutputPin,
    {
        let led = SmartLedsAdapter::new(channel, pin, rmt_buffer);

        Self {
            led,
            color: Hsv {
                hue: 0,
                sat: 255,
                val: 255,
            },
            brightness_level: 10,
        }
    }

    fn update(
        &mut self,
    ) -> Result<(), <SmartLedsAdapter<ChannelType, BUFFER_SIZE> as SmartLedsWrite>::Error> {
        let data = [hsv2rgb(self.color)];
        self.led.write(brightness(
            gamma(data.iter().cloned()),
            self.brightness_level,
        ))
    }

    pub fn set_hue(
        &mut self,
        hue: u8,
    ) -> Result<(), <SmartLedsAdapter<ChannelType, BUFFER_SIZE> as SmartLedsWrite>::Error> {
        self.color.hue = hue;
        self.update()
    }

    pub fn set_rgb(
        &mut self,
        r: u8,
        g: u8,
        b: u8,
        brightness_level: u8,
    ) -> Result<(), <SmartLedsAdapter<ChannelType, BUFFER_SIZE> as SmartLedsWrite>::Error> {
        let data = [RGB8::new(r, g, b)];
        self.led
            .write(brightness(gamma(data.iter().cloned()), brightness_level))
    }

    pub fn set_brightness(
        &mut self,
        brightness: u8,
    ) -> Result<(), <SmartLedsAdapter<ChannelType, BUFFER_SIZE> as SmartLedsWrite>::Error> {
        self.brightness_level = brightness;
        self.update()
    }
}
