# Supported devices
Depending on the device, two levels of support are available. `Basic Support` devices support a limited subset of features that are common across devices, whereas `Full Support` devices can be completely controlled.

This software can also control these devices if they are connected to a `WI-DG`.

## Full support
These devices support the full feature set (input, output, routing, peqs, etc.)

### Devices
- miniDSP 2x4HD
- miniDSP Flex
- miniDSP Flex DL
- DDRC-24
- DDRC-88A/D
- miniSHARC series
- miniDSP 2x8/8x8/4x10/10x10
- nanoDIGI 2x8
- SHD series
- C-DSP 8x12

## Basic support
For these devices, only the following settings can be changed:
- Master Gain
- Master Mute
- Input Source
- Active Configuration Presets
- Dirac Live Status

### Devices
- miniDSP 2x4HD (48kHz), hardware ID 10 / DSP ID 99
- C-DSP 6x8 
- nanoSHARC series
- OpenDRC series 
- nanoAVR HD/HDA

The 48 kHz 2x4HD profile identifies the device and supports the common master
controls with the correct Analog/Toslink/USB input names. Its internal sample
rate is 48000 Hz. Channel controls, meters, PEQ, crossovers, delay and FIR are
not enabled until a 48 kHz DSP parameter map has been verified; the 96 kHz
profile's addresses are not reused.

# Adding support for new devices
If you have a device that is on the `Basic Support` tier, you can help adding support by inspecting the commands sent by the plugin application.

TODO: Add guide for adding new devices
