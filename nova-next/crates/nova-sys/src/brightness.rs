//! Screen brightness through WMI (laptop panels only); -1 when the screen has no software
//! brightness (Brightness in NovaHelper.cs). Security is set on this connection only
//! (CoSetProxyBlanket), never process-wide, so WebView2 and the rest of the app are unaffected.
use windows::{
    core::{BSTR, PCWSTR},
    Win32::System::{
        Com::{CoCreateInstance, CoSetProxyBlanket, CLSCTX_INPROC_SERVER, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE},
        Variant::{VariantToInt32, VariantToStringAlloc, VARIANT},
        Wmi::{IWbemClassObject, IWbemLocator, IWbemServices, WbemLocator, WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_INFINITE},
    },
};

fn services() -> windows::core::Result<IWbemServices> {
    unsafe {
        let loc: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER)?;
        let svc = loc.ConnectServer(&BSTR::from("root\\WMI"), &BSTR::new(), &BSTR::new(), &BSTR::new(), 0, &BSTR::new(), None)?;
        // RPC_C_AUTHN_WINNT / RPC_C_AUTHZ_NONE
        CoSetProxyBlanket(&svc, 10, 0, PCWSTR::null(), RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE, None, EOAC_NONE)?;
        Ok(svc)
    }
}

fn first(svc: &IWbemServices, query: &str) -> windows::core::Result<Option<IWbemClassObject>> {
    unsafe {
        let en = svc.ExecQuery(&BSTR::from("WQL"), &BSTR::from(query), WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY, None)?;
        let mut objs = [None];
        let mut n = 0u32;
        let _ = en.Next(WBEM_INFINITE, &mut objs, &mut n);
        Ok(if n > 0 { objs[0].take() } else { None })
    }
}

pub fn get() -> i64 {
    let read = || -> windows::core::Result<i64> {
        let svc = services()?;
        let Some(o) = first(&svc, "SELECT CurrentBrightness FROM WmiMonitorBrightness")? else { return Ok(-1) };
        let mut v = VARIANT::default();
        unsafe { o.Get(windows::core::w!("CurrentBrightness"), 0, &mut v, None, None)? };
        Ok(unsafe { VariantToInt32(&v) }.map(i64::from).unwrap_or(-1))
    };
    read().unwrap_or(-1)
}

pub fn set(v: i64) {
    let write = || -> windows::core::Result<()> {
        let svc = services()?;
        let Some(inst) = first(&svc, "SELECT * FROM WmiMonitorBrightnessMethods")? else { return Ok(()) };
        unsafe {
            let mut path = VARIANT::default();
            inst.Get(windows::core::w!("__PATH"), 0, &mut path, None, None)?;
            let p = VariantToStringAlloc(&path)?;
            let path = BSTR::from(p.to_string().unwrap_or_default());
            windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as _));
            let mut class = None;
            svc.GetObject(&BSTR::from("WmiMonitorBrightnessMethods"), Default::default(), None, Some(&mut class), None)?;
            let class = class.ok_or(windows::core::Error::empty())?;
            let mut in_def = None;
            class.GetMethod(windows::core::w!("WmiSetBrightness"), 0, &mut in_def, std::ptr::null_mut())?;
            let params = in_def.ok_or(windows::core::Error::empty())?.SpawnInstance(0)?;
            params.Put(windows::core::w!("Timeout"), 0, &VARIANT::from(1i32), 0)?;
            params.Put(windows::core::w!("Brightness"), 0, &VARIANT::from(v.clamp(0, 100) as u8), 0)?;
            svc.ExecMethod(&path, &BSTR::from("WmiSetBrightness"), Default::default(), None, &params, None, None)?;
        }
        Ok(())
    };
    let _ = write();
}
