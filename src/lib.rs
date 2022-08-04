#![no_std]

use core::convert::Infallible;
use core::fmt;
use core::ops::Deref;

use embedded_hal::spi;

/// The command byte. This should be set as the first byte of the transfer to the DAC
///
///  DC  DC  DC  DC
/// B23 B22 B21 B20 B19 B18 B17 B16 REGISTER     HEX
///  0   0   0   0   0   0   0   0   NOOP        0x00
///  0   0   0   0   0   0   0   1   DEVID       0x01
///  0   0   0   0   0   0   1   1   SYNC        0x02
///  0   0   0   0   0   0   1   1   CONFIG      0x03
///  0   0   0   0   0   1   0   0   GAIN        0x04
///  0   0   0   0   0   1   0   1   TRIGGER     0x05
///  0   0   0   0   0   1   1   1   STATUS      0x07
///  0   0   0   0   1   0   0   0   DACDATA     0x08
#[allow(clippy::upper_case_acronyms, dead_code)]
enum Command {
    NOOP,
    DEVID,
    SYNC,
    CONFIG,
    GAIN,
    TRIGGER,
    STATUS,
    DACDATA,
}
impl Deref for Command {
    type Target = u8;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::NOOP => &0x00,
            Self::DEVID => &0x01,
            Self::SYNC => &0x02,
            Self::CONFIG => &0x03,
            Self::GAIN => &0x04,
            Self::TRIGGER => &0x05,
            Self::STATUS => &0x07,
            Self::DACDATA => &0x08,
        }
    }
}

#[derive(Default)]
struct DacState {
    config: DacConfig,
    gain: GainConfig,
}

#[derive(Default)]
struct DacConfig {
    ref_pwdwn: InternRefState,
    dac_pwdwn: PowerState,
}
impl DacConfig {
    fn to_array(&self) -> [u8; 2] {
        [
            // When set to 1, this bit disables the device internal reference.
            matches!(self.ref_pwdwn, InternRefState::Disable) as u8,
            // When set to 1, the DAC in power-down mode and the DAC output is connected to GND
            // through a 1-kΩ internal resistor.
            matches!(self.dac_pwdwn, PowerState::Off) as u8,
        ]
    }
}

struct GainConfig {
    ref_div: RefDivState,
    buff_gain: GainState,
}
impl Default for GainConfig {
    fn default() -> Self {
        Self {
            ref_div: RefDivState::OneX,
            buff_gain: GainState::TwoX,
        }
    }
}
impl GainConfig {
    fn to_array(&self) -> [u8; 2] {
        [
            // When REF-DIV set to 1, the reference voltage is internally divided by a factor of 2.
            matches!(self.ref_div, RefDivState::Half) as u8,
            // When set to 1, the buffer amplifier for corresponding DAC has a gain of 2.
            matches!(self.buff_gain, GainState::TwoX) as u8,
        ]
    }
}

/// The state of the dac output
pub enum PowerState {
    On,
    Off,
}
impl Default for PowerState {
    fn default() -> Self {
        Self::On
    }
}

/// The state of the dac gain
pub enum GainState {
    TwoX,
    OneX,
}
impl Default for GainState {
    fn default() -> Self {
        Self::TwoX
    }
}

/// The state of the DAC reference divider which applies to both the internal and external
/// reference
pub enum RefDivState {
    Half,
    OneX,
}
impl Default for RefDivState {
    fn default() -> Self {
        Self::OneX
    }
}

// The state of the DAC internal Reference
pub enum InternRefState {
    Disable,
    Enable,
}
impl Default for InternRefState {
    fn default() -> Self {
        Self::Enable
    }
}

#[derive(PartialEq, Eq)]
/// The state of the DAC alarm which is high when there is not enough headroom between Vdd and the
/// reference
pub enum AlarmStatus {
    High,
    Low,
}

#[derive(Debug)]
/// The error for this crate. A spi error can be returned by the HAL for every transfer or a bad
/// value can be returned if the requested output value is too large for the chosen DAC.
pub enum DacError {
    BadValue,
    SpiError,
}
impl fmt::Display for DacError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadValue => f.write_str("Bad Value"),
            Self::SpiError => f.write_str("Internal HAL SPI error"),
        }
    }
}
impl From<&dyn embedded_hal::spi::Error> for DacError {
    fn from(_: &dyn embedded_hal::spi::Error) -> Self {
        DacError::SpiError
    }
}

impl From<Infallible> for DacError {
    fn from(_: Infallible) -> Self {
        DacError::SpiError
    }
}

macro_rules! Dac {
    (  $Name:ident, $bits:expr) => {
        pub struct $Name<Spi> {
            spi: Spi,
            data: [u8; 3],
            dac_state: DacState,
        }

        impl<Spi> $Name<Spi>
        where
            Spi: spi::blocking::SpiDevice,
            Spi::Bus: spi::blocking::SpiBusWrite,
            DacError: core::convert::From<<Spi as embedded_hal::spi::ErrorType>::Error>,
        {
            pub fn new(spi: Spi) -> Self {
                Self {
                    spi,
                    data: [0x08u8, 0, 0],
                    dac_state: DacState::default(),
                }
            }

            // Set the output voltage of the DAC without checking the level bounds for the dac
            pub fn set_output_level_unckecked(&mut self, level: u16) -> Result<(), DacError> {
                // Data are MSB aligned in straight binary format
                self.data[0] = *Command::DACDATA;
                self.data[1..].copy_from_slice(level.to_be_bytes().as_slice());
                self.spi.write(&self.data).map_err(DacError::from)?;
                Ok(())
            }

            // Set the output voltage of the DAC and check the level bounds for the
            pub fn set_output_level(&mut self, level: u16) -> Result<(), DacError> {
                // Data are MSB aligned in straight binary format
                if level as u32 & (1u32 << $bits) > 0 {
                    return Err(DacError::BadValue);
                }
                self.data[0] = *Command::DACDATA;
                self.data[1..].copy_from_slice(level.to_be_bytes().as_slice());
                self.spi.write(&self.data).map_err(DacError::from)?;
                Ok(())
            }

            /// Enables and disables the device internal reference. The internal reference is on by default
            pub fn set_internal_reference(
                &mut self,
                intern_ref: InternRefState,
            ) -> Result<(), DacError> {
                self.dac_state.config.ref_pwdwn = intern_ref;
                self.data[0] = *Command::CONFIG;
                self.data[1..].copy_from_slice(&self.dac_state.config.to_array());
                self.spi.write(&self.data).map_err(DacError::from)?;
                Ok(())
            }

            /// In power-off state the DAC output is connected to GND through a 1-kΩ internal resistor. The
            /// device is in power `On` state by default
            pub fn set_power_state(&mut self, state: PowerState) -> Result<(), DacError> {
                self.dac_state.config.dac_pwdwn = state;
                self.data[0] = *Command::CONFIG;
                self.data[1..].copy_from_slice(&self.dac_state.config.to_array());
                self.spi.write(&self.data).map_err(DacError::from)?;
                Ok(())
            }

            /// The reference voltage to the device (either from the internal or external reference) can be
            /// divided by a factor of two by setting the reference divider to `Half`. Make sure to configure
            /// the reference divider so that there is sufficient headroom from VDD to the DAC operating
            /// reference voltage. Improper configuration of the reference divider triggers a reference
            /// alarm condition. In the case of an alarm condition, the reference buffer is shut down, and
            /// all the DAC outputs go to 0 V. The DAC data registers are unaffected by the alarm
            /// condition, and thus enable the DAC output to return to normal operation after the reference
            /// divider is configured correctly. When the reference divider is set to `Half`, the reference
            /// voltage is internally divided by a factor of 2. The reference divider is set to `OneX` by
            /// default
            pub fn set_reference_divider(&mut self, ref_div: RefDivState) -> Result<(), DacError> {
                self.dac_state.gain.ref_div = ref_div;
                self.data[0] = *Command::GAIN;
                self.data[1..].copy_from_slice(&self.dac_state.gain.to_array());
                self.spi.write(&self.data).map_err(DacError::from)?;
                Ok(())
            }

            /// When set to `TwoX`, the buffer amplifier for the DAC has a gain of 2x doubling the
            /// voltage output. When set to `OneX` it has a gain of 1x. Using this gain can be
            /// especially useful when using the internal reference divider set to `Half`. The
            /// output gain is set to `TwoX` by default
            pub fn set_output_gain(&mut self, gain: GainState) -> Result<(), DacError> {
                self.dac_state.gain.buff_gain = gain;
                self.data[0] = *Command::GAIN;
                self.data[1..].copy_from_slice(&self.dac_state.gain.to_array());
                self.spi.write(&self.data).map_err(DacError::from)?;
                Ok(())
            }
        }

        impl<Spi> $Name<Spi>
        where
            Spi: spi::blocking::SpiDevice,
            Spi::Bus: spi::blocking::SpiBusRead,
            DacError: core::convert::From<<Spi as embedded_hal::spi::ErrorType>::Error>,
        {
            /// `AlarmStatus` is `High` when the difference between the reference and supply pins is below a minimum
            /// analog threshold. The status is `Low` otherwise. When `High`, the reference buffer is shut down, and the DAC
            /// outputs are all zero volts. The DAC codes are unaffected, and the DAC output returns to
            /// normal when the difference is above the analog threshold.
            pub fn ref_alarm_status(&mut self) -> Result<AlarmStatus, DacError> {
                self.data[0] = *Command::STATUS;
                self.data[1] = 0;
                self.data[2] = 0;
                self.spi.read(&mut self.data).map_err(DacError::from)?;
                if self.data[2] == 1 {
                    Ok(AlarmStatus::High)
                } else {
                    Ok(AlarmStatus::Low)
                }
            }
        }
    };
}

Dac!(Dac80501, 16);
Dac!(Dac70501, 14);
Dac!(Dac60501, 12);
