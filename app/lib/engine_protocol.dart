import 'dart:convert';
import 'dart:typed_data';

import 'core/destination_controls.dart';
import 'core/local_library.dart';

class Cmds {
  static String consoleControls(String requestId, String path) => jsonEncode({
        'cmd': 'console_controls', 'request_id': requestId, 'path': path,
      });
  static String consoleControlSet(String requestId, int epoch, String path,
          Object? value, Object revision) => jsonEncode({
        'cmd': 'console_control_set', 'request_id': requestId, 'epoch': epoch,
        'path': path, 'value': value, 'revision': revision,
      });
  static String connectSim() => '{"cmd":"connect","driver":"sim"}';
  static String connect([String driver = 'sim', String? addr]) => addr == null
      ? '{"cmd":"connect","driver":${_q(driver)}}'
      : '{"cmd":"connect","driver":${_q(driver)},"addr":${_q(addr)}}';
  static String scan() => '{"cmd":"scan"}';
  static String refreshDevices(String requestId, List<String> hosts) =>
      jsonEncode(
          {'cmd': 'refresh_devices', 'request_id': requestId, 'hosts': hosts});

  /// Local storage only; does not probe, connect, or disconnect a mixer.
  static String forgetSavedMixers(String requestId) {
    if (requestId.isEmpty) throw ArgumentError.value(requestId, 'requestId');
    return jsonEncode({'cmd': 'forget_saved_mixers', 'request_id': requestId});
  }

  static String discoverDevices(String requestId) =>
      jsonEncode({'cmd': 'discover_devices', 'request_id': requestId});
  static String probe(String addr) => '{"cmd":"probe","addr":${_q(addr)}}';
  static String probeDesk(String addr) => probe(addr);
  static String connectDevice(String id) =>
      '{"cmd":"connect_device","id":${_q(id)}}';
  static String disconnect() => '{"cmd":"disconnect"}';
  static String info() => '{"cmd":"info"}';
  static String stats() => '{"cmd":"stats"}';
  static String diagnostics([int offset = 0]) =>
      jsonEncode({'cmd': 'diagnostics', 'offset': offset});
  static String hello([bool binaryMeters = true, bool? taps]) => taps == null
      ? '{"cmd":"hello","binary_meters":$binaryMeters}'
      : '{"cmd":"hello","binary_meters":$binaryMeters,"taps":$taps}';
  static String fader(int ch, double value, [String? key]) =>
      '{"cmd":"fader","ch":$ch,"value":$value${key == null ? '' : ',"key":${_q(key)}'}}';
  static String mute(int ch, bool muted, [String? key]) =>
      '{"cmd":"mute","ch":$ch,"muted":$muted${key == null ? '' : ',"key":${_q(key)}'}}';
  static String mainFader(double value) =>
      '{"cmd":"main_fader","value":$value}';
  static String mainMute(bool muted) => '{"cmd":"main_mute","muted":$muted}';
  static String solo(int ch, bool soloed) =>
      '{"cmd":"solo","ch":$ch,"soloed":$soloed}';
  static String clearSolo() => '{"cmd":"clear_solo"}';
  static String param(int ch, String field, Object value) =>
      '{"cmd":"param","ch":$ch,"field":${_q(field)},"value":${jsonEncode(value)}}';
  static String destinationParam(
          String requestId, DestinationControlVm target, double value,
          {List<DestinationControlVm>? guard}) =>
      jsonEncode({
        'cmd': 'destination_param',
        if (guard != null) 'guard': guard.map((c) => c.descriptor).toList(),
        'request_id': requestId,
        'target': target.target,
        'control': target.id,
        'epoch': target.epoch,
        'expected': target.wireValue(target.value ?? double.nan) ??
            (throw ArgumentError.value(target.value, 'expected',
                'Destination value is outside its wire domain')),
        'value': target.wireValue(value) ??
            (throw ArgumentError.value(value, 'value',
                'Destination value is outside its wire domain')),
      });
  static String paramGuarded(
          int ch, String field, Object value, Map<String, Object> expect) =>
      jsonEncode({
        'cmd': 'param_guarded',
        'ch': ch,
        'field': field,
        'value': value,
        'expect': expect
      });
  static String spectrumTarget(int? ch) =>
      '{"cmd":"spectrum_target","ch":${ch ?? 'null'}}';
  static String focus(List<int> chs) => chs.length == 1
      ? '{"cmd":"focus","ch":${chs.first}}'
      : '{"cmd":"focus","chs":[${chs.join(',')}]}';
  static String lock(int ch) => '{"cmd":"lock","ch":$ch}';
  static String unlock(int ch) => '{"cmd":"unlock","ch":$ch}';
  static String listShows() => '{"cmd":"list_shows"}';
  static String localLibraryStatus() => '{"cmd":"local_library_status"}';
  static String localLibraryMutate(
      String requestId, int expectedRevision, LocalLibraryAction action) {
    if (requestId.isEmpty || expectedRevision < 0) {
      throw ArgumentError('A request id and current revision are required');
    }
    return jsonEncode({
      'cmd': 'local_library_mutate',
      'request_id': requestId,
      'expected_revision': expectedRevision,
      'action': action.toJson(),
    });
  }

  static String captureScene(String show, String name) =>
      '{"cmd":"capture_scene","show":${_q(show)},"name":${_q(name)}}';
  static String recallScene(String show, String name, [double? fadeMs]) =>
      '{"cmd":"recall_scene","show":${_q(show)},"name":${_q(name)},"fade_ms":${fadeMs ?? 'null'}}';
  static String deleteScene(String show, String name) =>
      '{"cmd":"delete_scene","show":${_q(show)},"name":${_q(name)}}';
  static String sceneSave(int n, String name) =>
      '{"cmd":"scene_save","n":$n,"name":${_q(name)}}';
  static String sceneRecall(int n, [double? fadeMs]) =>
      '{"cmd":"scene_recall","n":$n${fadeMs == null ? '' : ',"fade_ms":$fadeMs'}}';
  static String sceneRename(int n, String name) =>
      '{"cmd":"scene_rename","n":$n,"name":${_q(name)}}';
  static String sceneDelete(int n) => '{"cmd":"scene_delete","n":$n}';
  static String sceneClear() => '{"cmd":"scene_clear"}';
  static String fxSetType(int slot, String type) =>
      '{"cmd":"fx_set_type","slot":$slot,"type":${_q(type)}}';
  static String fxParam(int slot, String id, double value) =>
      '{"cmd":"fx_param","slot":$slot,"id":${_q(id)},"value":$value}';
  static String fxBypass(int slot, bool bypass) =>
      '{"cmd":"fx_bypass","slot":$slot,"bypass":$bypass}';
  static String fxOn(int slot, bool on) =>
      '{"cmd":"fx_on","slot":$slot,"on":$on}';
  static String patchSet(int strip, String group, int index,
          {String target = ''}) =>
      '{"cmd":"patch_set","strip":$strip,"group":${_q(group)},"index":$index,"target":${_q(target)}}';
  static String patchClear(int strip, {String target = ''}) =>
      '{"cmd":"patch_clear","strip":$strip,"target":${_q(target)}}';
  static String offline(String model) =>
      '{"cmd":"offline","model":${_q(model)}}';
  static String offlinePush(bool confirm) =>
      '{"cmd":"offline_push","confirm":$confirm}';
  static String offlineDiscard() => '{"cmd":"offline_discard"}';
  static String busFader(int bus, double value) =>
      '{"cmd":"bus_fader","bus":$bus,"value":$value}';
  static String busMute(int bus, bool muted) =>
      '{"cmd":"bus_mute","bus":$bus,"muted":$muted}';
  static String subgroupFader(int group, double value) =>
      '{"cmd":"subgroup_fader","group":$group,"value":$value}';
  static String subgroupMute(int group, bool muted) =>
      '{"cmd":"subgroup_mute","group":$group,"muted":$muted}';
  static String subgroupAssign(int ch, int group, bool member) =>
      '{"cmd":"subgroup_assign","ch":$ch,"group":$group,"member":$member}';
  static String muteGroup(int group, bool muted) =>
      '{"cmd":"mute_group","group":$group,"muted":$muted}';
  static String muteGroupAssign(int ch, int group, bool member) =>
      '{"cmd":"mute_group_assign","ch":$ch,"group":$group,"member":$member}';
  static String fileDownload(String filename) =>
      '{"cmd":"file_download","filename":${_q(filename)}}';
  static String fileUpload(String filename, String data, int fileType) =>
      '{"cmd":"file_upload","filename":${_q(filename)},"data":${_q(data)},"file_type":$fileType}';

  static String _q(String s) =>
      '"${s.replaceAll('\\', '\\\\').replaceAll('"', '\\"')}"';
}

// ---------------------------------------------------------------------------
// EngineApi: transport behind the UI. `FfiEngine` drives the real embedded
// engine; the sim engine drives the same Cmd/Event JSON in memory.
// ---------------------------------------------------------------------------
abstract class EngineApi {
  Future<String> boot();
  Future<void> cmd(String clientId, String json);
  Future<List<String>> poll(String clientId, int cap);
  // Hot meter path: length-prefixed binary frames; non-meter events stay
  // on [poll] (JSON fallback, never lost).
  Future<Uint8List> pollBinary(String clientId, int cap);
  Future<void> resync(String clientId);
}
