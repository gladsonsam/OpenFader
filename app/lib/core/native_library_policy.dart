/// Provider-specific descriptions of native operations. The fallback makes no
/// claim about file formats, readback, or the effects of an unfamiliar desk.
abstract class NativeLibraryPolicy {
  const NativeLibraryPolicy();
  String get refreshTitle => 'Refresh provider reports?';
  String get refreshDescription =>
      'Request the current provider library reports. Current mixer edits and restoration are not verified.';
  String get replaceLabel => 'Replace';
  String replaceDescription(bool mixer) => mixer
      ? 'Replace this provider snapshot with current mixer settings. Coverage and audio state are not verified. No automatic retry.'
      : 'Overwrite this memory-only capture with current simulated fader, mute, pan and name values. No native file is created.';
  String deleteDescription(bool mixer) => mixer
      ? 'Delete this provider snapshot permanently. Refresh before another operation. No automatic retry.'
      : 'Delete this memory-only simulated strip capture? It cannot be undone.';
}

class XairNativeLibraryPolicy extends NativeLibraryPolicy {
  const XairNativeLibraryPolicy();
  @override
  String get refreshTitle => 'Read snapshot names and scopes?';
  @override
  String get refreshDescription =>
      'Read the names and recall scopes of all 64 mixer snapshot slots.';
  @override
  String get replaceLabel => 'Save to slot';
  @override
  String replaceDescription(bool mixer) =>
      'Save current mixer settings to this slot, replacing its contents. The mixer’s snapshot scope applies.';
  @override
  String deleteDescription(bool mixer) =>
      'Delete this mixer snapshot. Refresh the slot names before another change.';
}

class Dl32sNativeLibraryPolicy extends NativeLibraryPolicy {
  const Dl32sNativeLibraryPolicy();
  @override
  String get refreshTitle => 'Download current show?';
  @override
  String get refreshDescription =>
      'Download and inspect the stored-show ZIP. This does not save a file or certify current unsaved edits or native restoration. The stored snapshot index is refreshed; current edits remain unverified.';
  @override
  String replaceDescription(bool mixer) => mixer
      ? 'Store current mixer state over this user position. The provider verifies stored files by readback. Current-edit coverage and safes remain unverified. No automatic retry.'
      : super.replaceDescription(mixer);
  @override
  String deleteDescription(bool mixer) => mixer
      ? 'Delete this stored snapshot permanently. Later native positions compact; refresh before another operation. No automatic retry.'
      : super.deleteDescription(mixer);
}

class GenericNativeLibraryPolicy extends NativeLibraryPolicy {
  const GenericNativeLibraryPolicy();
}

NativeLibraryPolicy nativeLibraryPolicy(Object? provider) => switch (provider) {
      'xair' => const XairNativeLibraryPolicy(),
      'dl32s' => const Dl32sNativeLibraryPolicy(),
      _ => const GenericNativeLibraryPolicy(),
    };
