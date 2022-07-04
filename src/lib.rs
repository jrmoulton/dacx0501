#![no_std]

use embedded_hal::spi;

/// An 80501 DAC with 16 bit resolution.
/// Currently this dac requires that a device be able to write and flush a bus.
/// The flushing capability is not currenlty being used but for future
/// compatability we will keep it until certain that flushing is guarenteed to
/// be unnecessary.
pub struct Dac80501<Spi>
where
    Spi: spi::blocking::SpiBusWrite + spi::blocking::SpiBusFlush,
{
    spi: Spi,
    data: [u8; 3],
}

impl<Spi> Dac80501<Spi>
where
    Spi: spi::blocking::SpiBusWrite + spi::blocking::SpiBusFlush,
{
    pub fn new(spi: Spi) -> Self {
        Self {
            spi,
            data: [0x08u8, 0, 0],
        }
    }

    pub fn set_output_level(&mut self, level: u16) -> Result<(), <Spi as spi::ErrorType>::Error> {
        self.data[1..].copy_from_slice(level.to_be_bytes().as_slice());
        self.spi.write(&self.data)?;
        Ok(())
    }
}
