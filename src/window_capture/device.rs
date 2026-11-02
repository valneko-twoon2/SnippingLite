use windows::{
    core::{Error, Result},
    Win32::{
        Foundation::{E_FAIL, HMODULE},
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP},
            Direct3D11::*,
        },
    },
};

/// D3D11 のデバイス。GPU がないマシン（仮想マシンや CI）では、ハードウェアで作れないので、
/// ソフトウェアの描画（WARP）で作る。窓の撮影は、画面の絵をそのまま受け取るだけなので、
/// WARP でも足りる。
pub(super) fn create_device() -> Result<(ID3D11Device, ID3D11DeviceContext)> {
    let create = |driver: D3D_DRIVER_TYPE| {
        let (mut device, mut context) = (None, None);
        // SAFETY: the out pointers outlive the call.
        unsafe {
            D3D11CreateDevice(
                None,
                driver,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
        }?;
        match (device, context) {
            (Some(device), Some(context)) => Ok((device, context)),
            _ => Err(Error::from_hresult(E_FAIL)),
        }
    };
    create(D3D_DRIVER_TYPE_HARDWARE).or_else(|_| create(D3D_DRIVER_TYPE_WARP))
}
