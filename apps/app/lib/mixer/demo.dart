// Live faders, meters and processing parameters come from the engine.
class MixerDemoStrip {
  final String name;
  final double value, pan;
  final List<double> compression;
  const MixerDemoStrip(
      {required this.name,
      required this.value,
      required this.pan,
      required this.compression});
}

const mixerDemoStrips = <MixerDemoStrip>[
  MixerDemoStrip(
      name: "Bass", value: 0.633721, pan: 50.0, compression: [0.0, 43.0]),
  MixerDemoStrip(
      name: "Tom", value: 0.420543, pan: 81.0, compression: [100.0, 0.0]),
  MixerDemoStrip(
      name: "HiHat", value: 0.639535, pan: 36.0, compression: [0.0, 0.0]),
  MixerDemoStrip(
      name: "Kick In", value: 0.744186, pan: 50.0, compression: [0.0, 0.0]),
  MixerDemoStrip(
      name: "Kick Out",
      value: 0.5639529999999999,
      pan: 50.0,
      compression: [13.0, 17.0]),
  MixerDemoStrip(
      name: "OH", value: 0.286822, pan: 50.0, compression: [0.0, 0.0]),
  MixerDemoStrip(
      name: "Rack 1", value: 0.496124, pan: 81.0, compression: [7.0, 8.0]),
  MixerDemoStrip(
      name: "Rack 2", value: 0.536822, pan: 66.0, compression: [100.0, 0.0]),
  MixerDemoStrip(
      name: "Main", value: 0.7635660000000001, pan: 50.0, compression: [0.0]),
];
