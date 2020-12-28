#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(improper_ctypes)]
#![allow(dead_code)]
mod bindings;
use bindings::*;

const MAIN_MEMORY: u64 =  0x80000000;

#[derive(Clone, Debug)]
pub enum Error {
    Error
}

fn hv_call(code: hv_return_t) -> Result<(), Error> {
    match code {
	HV_SUCCESS => Ok(()),
	_ => Err(Error::Error),
    }
}

unsafe fn set_mem() -> Result<(), Error> {
    let  mem = {
	libc::mmap(
	    std::ptr::null_mut(),
	    1024 * 1024 * 256,
	    libc::PROT_READ | libc::PROT_WRITE,
	    libc::MAP_PRIVATE | libc::MAP_ANON | libc::MAP_NORESERVE,
	    -1,
	    0,
	)
    };

    if mem == libc::MAP_FAILED {
	panic!("mmap");
    }

    let code: Vec<u8> = vec![
	0x40, 0x00, 0x80, 0xD2, // mov x0, #2
	0x00, 0x08, 0x00, 0x91, // add x0, x0, #2
	0x00, 0x04, 0x00, 0xD1, // sub x0, x0, #1
	0x03, 0x00, 0x00, 0xD4, // smc #0
	0x02, 0x00, 0x00, 0xD4, // hvc #0
	0x00, 0x00, 0x20, 0xD4, // brk #0
    ];
    
    libc::memcpy(mem, code.as_ptr() as *const _ as *const libc::c_void, code.len());
    
    hv_call(hv_vm_create(std::ptr::null_mut()))?;
    hv_call(hv_vm_map(mem, MAIN_MEMORY, 0x1000000, (HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC).into()))?;

    Ok(())
}

struct Vcpu<'a> {
    vcpuid: hv_vcpu_t,
    vcpu_exit: &'a hv_vcpu_exit_t,
}

impl<'a> Vcpu<'a> {
    unsafe fn new() -> Result<Self, Error> {
	let mut vcpuid: hv_vcpu_t = 0;
	let vcpu_exit_ptr: *mut hv_vcpu_exit_t = std::ptr::null_mut();
  	
	hv_call(hv_vcpu_create(&mut vcpuid, &vcpu_exit_ptr as *const _ as *mut *mut _, std::ptr::null_mut()))?;
	hv_call(hv_vcpu_set_reg(vcpuid, hv_reg_t_HV_REG_CPSR, 0x3c4))?;
	hv_call(hv_vcpu_set_reg(vcpuid, hv_reg_t_HV_REG_PC, MAIN_MEMORY))?;

	let pc: u64 = 0;
	hv_call(hv_vcpu_get_reg(vcpuid, hv_reg_t_HV_REG_PC, &pc as *const _ as *mut _))?;
	println!("pc is {:x}", pc);

	hv_call(hv_vcpu_set_sys_reg(vcpuid, hv_sys_reg_t_HV_SYS_REG_SP_EL0, MAIN_MEMORY + 0x4000))?;
	hv_call(hv_vcpu_set_sys_reg(vcpuid, hv_sys_reg_t_HV_SYS_REG_SP_EL1, MAIN_MEMORY + 0x8000))?;
	
	hv_call(hv_vcpu_set_trap_debug_exceptions(vcpuid, true))?;

	let vcpu_exit: &hv_vcpu_exit_t = vcpu_exit_ptr.as_mut().unwrap();

	Ok(Self {
	    vcpuid,
	    vcpu_exit
	})
    }

    unsafe fn run(&self) -> Result<(), Error> {
	loop  {
	    hv_call(hv_vcpu_run(self.vcpuid))?;

	    match self.vcpu_exit.reason {
		hv_exit_reason_t_HV_EXIT_REASON_EXCEPTION => {
		    let pc: u64 = 0;
		    hv_call(hv_vcpu_get_reg(self.vcpuid, hv_reg_t_HV_REG_PC, &pc as *const _ as *mut _))?;
		    
		    println!("exception with pc at {:x}", pc);
		    let syndrome = self.vcpu_exit.exception.syndrome;
		    let ec = (syndrome >> 26) & 0x3f;
		    
		    match ec {
			0x16 => println!("HVC call"),
			0x17 => {
			    println!("SMC call");
			    hv_call(hv_vcpu_set_reg(self.vcpuid, hv_reg_t_HV_REG_PC, pc + 4))?;
			},
			0x3c => {
			    println!("BRK call");
			    break;
			}
			_ => panic!("unexpected exception: {:x}", ec),
		    }
		    
		},
		_ => panic!("unexpected exit reason"),
	    }
	}

	Ok(())
    }
}

fn main() {
    unsafe { set_mem().expect("Failed to set up VM memory") };

    let vcpu = unsafe { Vcpu::new().expect("Error creating vCPU") };
    unsafe { vcpu.run().expect("Error running vCPU") };
}
