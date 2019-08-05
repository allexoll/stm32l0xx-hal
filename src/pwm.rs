use core::marker::PhantomData;
use core::ops::Deref;

use cortex_m::interrupt;

use crate::gpio::gpioa::{PA0, PA1, PA2, PA3};
use crate::gpio::{AltMode};
use crate::hal;
use crate::pac::{
    tim2,
    TIM2,
    TIM3,
};
use crate::rcc::Rcc;
use crate::time::Hertz;
use cast::{u16, u32};

#[cfg(feature = "stm32l0x2")]
use crate::gpio::{
    gpioa::{
        PA5,
        PA15,
    },
    gpiob::{
        PB3,
        PB10,
        PB11,
    },
};

#[cfg(any(feature = "stm32l072", feature = "stm32l082"))]
use crate::gpio::{
    gpioa::{
        PA6,
        PA7,
    },
    gpiob::{
        PB0,
        PB1,
        PB4,
        PB5,
    },
};

#[cfg(feature = "stm32l072")]
use crate::gpio::{
    gpioc::{
        PC6,
        PC7,
        PC8,
        PC9,
    },
    gpioe::{
        PE3,
        PE4,
        PE5,
        PE6,
        PE9,
        PE10,
        PE11,
        PE12,
    },
};


pub struct Timer<I> {
    _instance: I,

    pub channel1: Pwm<I, C1, Unassigned>,
    pub channel2: Pwm<I, C2, Unassigned>,
    pub channel3: Pwm<I, C3, Unassigned>,
    pub channel4: Pwm<I, C4, Unassigned>,
}

impl<I> Timer<I>
    where I: Instance
{
    pub fn new(timer: I, frequency: Hertz, rcc: &mut Rcc) -> Self {
        timer.enable(rcc);

        let clk = timer.clock_frequency(rcc);
        let freq = frequency.0;
        let ticks = clk / freq;
        let psc = u16((ticks - 1) / (1 << 16)).unwrap();
        let arr = u16(ticks / u32(psc + 1)).unwrap();
        timer.psc.write(|w| w.psc().bits(psc));
        timer.arr.write(|w| w.arr().bits(arr.into()));
        timer.cr1.write(|w| w.cen().set_bit());

        Self {
            _instance: timer,

            channel1: Pwm::new(),
            channel2: Pwm::new(),
            channel3: Pwm::new(),
            channel4: Pwm::new(),
        }
    }
}


pub trait Instance: Deref<Target=tim2::RegisterBlock> {
    fn ptr() -> *const tim2::RegisterBlock;
    fn enable(&self, _: &mut Rcc);
    fn clock_frequency(&self, _: &mut Rcc) -> u32;
}

macro_rules! impl_instance {
    (
        $(
            $name:ty,
            $apbXenr:ident,
            $apbXrstr:ident,
            $timXen:ident,
            $timXrst:ident,
            $apbX_clk:ident;
        )*
    ) => {
        $(
            impl Instance for $name {
                fn ptr() -> *const tim2::RegisterBlock {
                    Self::ptr()
                }

                fn enable(&self, rcc: &mut Rcc) {
                    rcc.rb.$apbXenr.modify(|_, w| w.$timXen().set_bit());
                    rcc.rb.$apbXrstr.modify(|_, w| w.$timXrst().set_bit());
                    rcc.rb.$apbXrstr.modify(|_, w| w.$timXrst().clear_bit());
                }

                fn clock_frequency(&self, rcc: &mut Rcc) -> u32 {
                    rcc.clocks.$apbX_clk().0
                }
            }
        )*
    }
}

impl_instance!(
    TIM2, apb1enr, apb1rstr, tim2en, tim2rst, apb1_clk;
    TIM3, apb1enr, apb1rstr, tim3en, tim3rst, apb1_clk;
);


pub trait Channel {
    fn disable(_: &tim2::RegisterBlock);
    fn enable(_: &tim2::RegisterBlock);
    fn get_duty(_: &tim2::RegisterBlock) -> u16;
    fn set_duty(_: &tim2::RegisterBlock, duty: u16);
}

macro_rules! impl_channel {
    (
        $(
            $name:ident,
            $ccxe:ident,
            $ccmr_output:ident,
            $ocxpe:ident,
            $ocxm:ident,
            $ccrx:ident;
        )*
    ) => {
        $(
            pub struct $name;

            impl Channel for $name {
                fn disable(tim: &tim2::RegisterBlock) {
                    tim.ccer.modify(|_, w| w.$ccxe().clear_bit());
                }

                fn enable(tim: &tim2::RegisterBlock) {
                    tim.$ccmr_output().modify(|_, w| {
                        w.$ocxpe().set_bit();
                        w.$ocxm().bits(0b110)
                    });
                    tim.ccer.modify(|_, w| w.$ccxe().set_bit());
                }

                fn get_duty(tim: &tim2::RegisterBlock) -> u16 {
                    // This cast to `u16` is fine. The type is already `u16`,
                    // but on STM32L0x2, the SVD file seems to be wrong about
                    // that (or the reference manual is wrong; but in any case,
                    // we only ever write `u16` into this field).
                    tim.$ccrx.read().ccr().bits() as u16
                }

                fn set_duty(tim: &tim2::RegisterBlock, duty: u16) {
                    tim.$ccrx.write(|w| w.ccr().bits(duty.into()));
                }
            }
        )*
    }
}

impl_channel!(
    C1, cc1e, ccmr1_output, oc1pe, oc1m, ccr1;
    C2, cc2e, ccmr1_output, oc2pe, oc2m, ccr2;
    C3, cc3e, ccmr2_output, oc3pe, oc3m, ccr3;
    C4, cc4e, ccmr2_output, oc4pe, oc4m, ccr4;
);


pub struct Pwm<I, C, State> {
    channel: PhantomData<C>,
    timer:   PhantomData<I>,
    _state:  State,
}

impl<I, C> Pwm<I, C, Unassigned> {
    fn new() -> Self {
        Self {
            channel: PhantomData,
            timer:   PhantomData,
            _state:  Unassigned,
        }
    }

    pub fn assign<P>(self, pin: P) -> Pwm<I, C, Assigned<P>>
        where P: Pin<I, C>
    {
        pin.setup();
        Pwm {
            channel: self.channel,
            timer:   self.timer,
            _state:  Assigned(pin),
        }
    }
}

impl<I, C, P> hal::PwmPin for Pwm<I, C, Assigned<P>>
    where
        I: Instance,
        C: Channel,
{
    type Duty = u16;

    fn disable(&mut self) {
        interrupt::free(|_|
            // Safe, as the read-modify-write within the critical section
            C::disable(unsafe { &*I::ptr() })
        )
    }

    fn enable(&mut self) {
        interrupt::free(|_|
            // Safe, as the read-modify-write within the critical section
            C::enable(unsafe { &*I::ptr() })
        )
    }

    fn get_duty(&self) -> u16 {
        // Safe, as we're only doing an atomic read.
        C::get_duty(unsafe { &*I::ptr() })
    }

    fn get_max_duty(&self) -> u16 {
        // Safe, as we're only doing an atomic read.
        let tim = unsafe { &*I::ptr() };

        // This cast to `u16` is fine. The type is already `u16`, but on
        // STM32L0x2, the SVD file seems to be wrong about that (or the
        // reference manual is wrong; but in any case, we only ever write `u16`
        // into this field).
        tim.arr.read().arr().bits() as u16
    }

    fn set_duty(&mut self, duty: u16) {
        // Safe, as we're only doing an atomic write.
        C::set_duty(unsafe { &*I::ptr() }, duty);
    }
}


pub trait Pin<I, C> {
    fn setup(&self);
}

macro_rules! impl_pin {
    (
        $(
            $instance:ty: (
                $(
                    $name:ident,
                    $channel:ty,
                    $alternate_function:ident;
                )*
            )
        )*
    ) => {
        $(
            $(
                impl<State> Pin<$instance, $channel> for $name<State> {
                    fn setup(&self) {
                        self.set_alt_mode(AltMode::$alternate_function);
                    }
                }
            )*
        )*
    }
}

impl_pin!(
    TIM2: (
        PA0, C1, AF2;
        PA1, C2, AF2;
        PA2, C3, AF2;
        PA3, C4, AF2;
    )
);

#[cfg(feature = "stm32l0x2")]
impl_pin!(
    TIM2: (
        PA5,  C1, AF5;
        PA15, C1, AF5;
        PB3,  C2, AF2;
        PB10, C3, AF2;
        PB11, C4, AF2;
    )
);

#[cfg(any(feature = "stm32l072", feature = "stm32l082"))]
impl_pin!(
    TIM3: (
        PA6, C1, AF2;
        PA7, C2, AF2;
        PB0, C3, AF2;
        PB1, C4, AF2;
        PB4, C1, AF2;
        PB5, C2, AF4;
    )
);

#[cfg(feature = "stm32l072")]
impl_pin!(
    TIM2: (
        PE9,  C1, AF0;
        PE10, C2, AF0;
        PE11, C3, AF0;
        PE12, C4, AF0;
    )
    TIM3: (
        PC6, C1, AF2;
        PC7, C2, AF2;
        PC8, C3, AF2;
        PC9, C4, AF2;
        PE3, C1, AF2;
        PE4, C2, AF2;
        PE5, C3, AF2;
        PE6, C4, AF2;
    )
);


/// Indicates that a PWM channel has not been assigned to a pin
pub struct Unassigned;

/// Indicates that a PWM channel has been assigned to the given pin
pub struct Assigned<P>(P);
