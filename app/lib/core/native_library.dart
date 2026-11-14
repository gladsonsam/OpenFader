// Provider library descriptors are ephemeral guards, never local backup data.
//
// A provider may additionally describe desk banks (`banks`), integer-argument
// forms (`copy_forms`), local archive kinds (`archive`), header lines
// (`headers`), a one-line availability note (`banks_note`) and refresh
// wording (`refresh_title`/`refresh_description`). The generic renderer
// consumes only these descriptors: ranges, labels, operation ids and
// argument contracts live provider-side. Anything malformed fails closed:
// `parse` returns null and the library reads unavailable.
class NativeBankAction {
  final String op;
  final String? promptTitle;
  final String? confirmTitle;
  final String? confirmMessage;
  final String? numberArg;
  const NativeBankAction({
    required this.op,
    this.promptTitle,
    this.confirmTitle,
    this.confirmMessage,
    this.numberArg,
  });
}

class NativeBankDescriptor {
  final String id;
  final String title;
  final String rowKey;
  final int slotMin;
  final int slotMax;
  final String? bank;
  final List<Map<String, dynamic>> rows;
  final NativeBankAction? save;
  final NativeBankAction? recall;
  final NativeBankAction? rename;
  final NativeBankAction? delete;
  final NativeBankAction? add;
  const NativeBankDescriptor({
    required this.id,
    required this.title,
    required this.rowKey,
    required this.slotMin,
    required this.slotMax,
    this.bank,
    required this.rows,
    this.save,
    this.recall,
    this.rename,
    this.delete,
    this.add,
  });
}

class NativeCopyField {
  final String key;
  final String label;
  final int min;
  final int max;
  const NativeCopyField({
    required this.key,
    required this.label,
    required this.min,
    required this.max,
  });
}

class NativeCopyForm {
  final String id;
  final String op;
  final String title;
  final String submitLabel;
  final List<NativeCopyField> fields;
  const NativeCopyForm({
    required this.id,
    required this.op,
    required this.title,
    required this.submitLabel,
    required this.fields,
  });
}

class NativeArchiveKind {
  final String id;
  final String label;
  final int? slotMin;
  final int? slotMax;
  const NativeArchiveKind({
    required this.id,
    required this.label,
    this.slotMin,
    this.slotMax,
  });
}

class NativeArchiveDescriptor {
  final String exportOp;
  final String inspectOp;
  final String importOp;
  final String importConfirmTitle;
  final String importConfirmMessage;
  final List<NativeArchiveKind> kinds;
  const NativeArchiveDescriptor({
    required this.exportOp,
    required this.inspectOp,
    required this.importOp,
    required this.importConfirmTitle,
    required this.importConfirmMessage,
    required this.kinds,
  });
}

class NativeLibraryVm {
  final Map<String, dynamic> guard;
  final List<NativeBankDescriptor> banks;
  final List<NativeCopyForm> copyForms;
  final NativeArchiveDescriptor? archive;
  NativeLibraryVm._(this.guard, this.banks, this.copyForms, this.archive);
  String get label => guard['label'] as String;
  String get authority => guard['authority'] as String;
  bool get connected => guard['connected'] == true;
  bool get complete => guard['complete'] == true;
  int get capacity => guard['capacity'] as int;
  String get capacityLabel => guard['capacity_label'] is String
      ? guard['capacity_label'] as String
      : '$capacity user slots';
  String get limitations => guard['limitations'] as String;
  List<Map<String, dynamic>> get slots =>
      (guard['slots'] as List).cast<Map<String, dynamic>>();
  String? get refreshTitle => guard['refresh_title'] as String?;
  String? get refreshDescription => guard['refresh_description'] as String?;
  List<String> get headers {
    final raw = guard['headers'];
    if (raw is! List) return const [];
    return raw.whereType<String>().toList();
  }

  String? get banksNote => guard['banks_note'] as String?;
  List<Map<String, dynamic>>? get scopeFields {
    final fields = guard['scope_fields'];
    if (fields is! List || fields.isEmpty) return null;
    final result = <Map<String, dynamic>>[];
    for (var i = 0; i < fields.length; i++) {
      final field = fields[i];
      if (field is! Map<String, dynamic> ||
          field['index'] != i ||
          field['label'] is! String ||
          (field['label'] as String).isEmpty ||
          field['group'] is! String ||
          (field['group'] as String).isEmpty) {
        return null;
      }
      result.add(field);
    }
    return result;
  }

  bool hasScope(Map<String, dynamic> slot) =>
      slot['scope'] is String &&
      scopeFields != null &&
      (slot['scope'] as String).length == scopeFields!.length &&
      RegExp(r'^[+-]+$').hasMatch(slot['scope'] as String);
  bool supports(String op) => (guard['operations'] as List).contains(op);
  static NativeLibraryVm? parse(Object? raw) {
    if (raw is! Map<String, dynamic> ||
        raw['label'] is! String ||
        !const ['mixer', 'simulation'].contains(raw['authority']) ||
        raw['connected'] is! bool ||
        raw['complete'] is! bool ||
        raw['capacity'] is! int ||
        (raw['capacity'] as int) < 1 ||
        raw['generation'] is! int ||
        raw['revision'] is! int ||
        raw['operations'] is! List ||
        raw['slots'] is! List ||
        raw['limitations'] is! String) {
      return null;
    }
    final operations = raw['operations'].cast<String>();
    final ids = <int>{};
    for (final row in raw['slots'] as List) {
      if (row is! Map<String, dynamic> ||
          row['slot'] is! int ||
          row['name'] is! String ||
          !ids.add(row['slot'] as int)) {
        return null;
      }
    }
    if (raw.containsKey('headers') &&
        (raw['headers'] is! List ||
            (raw['headers'] as List).length > 8 ||
            (raw['headers'] as List)
                .any((h) => h is! String || h.isEmpty || h.length > 256))) {
      return null;
    }
    if (raw.containsKey('banks_note') &&
        raw['banks_note'] != null &&
        (raw['banks_note'] is! String ||
            (raw['banks_note'] as String).isEmpty ||
            (raw['banks_note'] as String).length > 256)) {
      return null;
    }
    for (final key in ['refresh_title', 'refresh_description']) {
      if (raw.containsKey(key) &&
          raw[key] != null &&
          (raw[key] is! String ||
              (raw[key] as String).isEmpty ||
              (raw[key] as String).length > 256)) {
        return null;
      }
    }
    final banks = _parseBanks(raw['banks'], operations);
    if (banks == null) return null;
    final copyForms = _parseCopyForms(raw['copy_forms'], operations);
    if (copyForms == null) return null;
    final archive = _parseArchive(raw['archive'], operations);
    if (raw.containsKey('archive') &&
        raw['archive'] != null &&
        archive == null) {
      return null;
    }
    return NativeLibraryVm._(Map.unmodifiable(raw), banks, copyForms, archive);
  }

  static List<NativeBankDescriptor>? _parseBanks(
      Object? raw, List<String> operations) {
    if (raw == null) return const [];
    if (raw is! List || raw.length > 16) return null;
    final ids = <String>{};
    final result = <NativeBankDescriptor>[];
    for (final entry in raw) {
      if (entry is! Map<String, dynamic>) return null;
      final id = entry['id'];
      final title = entry['title'];
      final rowKey = entry['row_key'];
      final slotMin = entry['slot_min'];
      final slotMax = entry['slot_max'];
      final rows = entry['rows'];
      if (id is! String ||
          id.isEmpty ||
          !ids.add(id) ||
          title is! String ||
          title.isEmpty ||
          title.length > 128 ||
          (rowKey != 'slot' && rowKey != 'position') ||
          slotMin is! int ||
          slotMax is! int ||
          slotMin > slotMax ||
          rows is! List ||
          rows.length > 1024) {
        return null;
      }
      final bank = entry['bank'];
      if (bank != null && (bank is! String || bank.isEmpty)) return null;
      final parsedRows = <Map<String, dynamic>>[];
      for (final row in rows) {
        if (row is! Map<String, dynamic> ||
            row[rowKey] is! int ||
            row['name'] is! String) {
          return null;
        }
        final key = row[rowKey] as int;
        if (rowKey == 'position') {
          // Position rows carry the slot-domain value separately as
          // `number` (e.g. cue positions 0..N with packed numbers M..K):
          // the number must sit inside the argument domain while the
          // position itself is only required to be sane.
          if (row['number'] is! int ||
              key < 0 ||
              (row['number'] as int) < slotMin ||
              (row['number'] as int) > slotMax) {
            return null;
          }
        } else if (key < slotMin || key > slotMax) {
          return null;
        }
        parsedRows.add(row);
      }
      NativeBankAction? action(String key) {
        final spec = entry[key];
        if (spec == null) return null;
        if (spec is! Map<String, dynamic>) return null;
        final op = spec['op'];
        if (op is! String || op.isEmpty || !operations.contains(op)) {
          return null;
        }
        for (final text in [
          spec['prompt_title'],
          spec['confirm_title'],
          spec['confirm_message']
        ]) {
          if (text != null &&
              (text is! String || text.isEmpty || text.length > 256)) {
            return null;
          }
        }
        final numberArg = spec['number_arg'];
        if (numberArg != null && (numberArg is! String || numberArg.isEmpty)) {
          return null;
        }
        return NativeBankAction(
          op: op,
          promptTitle: spec['prompt_title'] as String?,
          confirmTitle: spec['confirm_title'] as String?,
          confirmMessage: spec['confirm_message'] as String?,
          numberArg: numberArg as String?,
        );
      }

      final save = action('save');
      final recall = action('recall');
      final rename = action('rename');
      final delete = action('delete');
      final add = action('add');
      if (save == null &&
          recall == null &&
          rename == null &&
          delete == null &&
          add == null) {
        return null;
      }
      result.add(NativeBankDescriptor(
        id: id,
        title: title,
        rowKey: rowKey,
        slotMin: slotMin,
        slotMax: slotMax,
        bank: bank as String?,
        rows: parsedRows,
        save: save,
        recall: recall,
        rename: rename,
        delete: delete,
        add: add,
      ));
    }
    return result;
  }

  static List<NativeCopyForm>? _parseCopyForms(
      Object? raw, List<String> operations) {
    if (raw == null) return const [];
    if (raw is! List || raw.length > 8) return null;
    final ids = <String>{};
    final result = <NativeCopyForm>[];
    for (final entry in raw) {
      if (entry is! Map<String, dynamic>) return null;
      final id = entry['id'];
      final op = entry['op'];
      final title = entry['title'];
      final submitLabel = entry['submit_label'];
      final fields = entry['fields'];
      if (id is! String ||
          id.isEmpty ||
          !ids.add(id) ||
          op is! String ||
          op.isEmpty ||
          !operations.contains(op) ||
          title is! String ||
          title.isEmpty ||
          title.length > 128 ||
          submitLabel is! String ||
          submitLabel.isEmpty ||
          submitLabel.length > 32 ||
          fields is! List ||
          fields.isEmpty ||
          fields.length > 8) {
        return null;
      }
      final parsedFields = <NativeCopyField>[];
      final keys = <String>{};
      for (final field in fields) {
        if (field is! Map<String, dynamic>) return null;
        final key = field['key'];
        final label = field['label'];
        final min = field['min'];
        final max = field['max'];
        if (key is! String ||
            key.isEmpty ||
            !keys.add(key) ||
            label is! String ||
            label.isEmpty ||
            label.length > 64 ||
            min is! int ||
            max is! int ||
            min > max) {
          return null;
        }
        parsedFields
            .add(NativeCopyField(key: key, label: label, min: min, max: max));
      }
      result.add(NativeCopyForm(
        id: id,
        op: op,
        title: title,
        submitLabel: submitLabel,
        fields: parsedFields,
      ));
    }
    return result;
  }

  static NativeArchiveDescriptor? _parseArchive(
      Object? raw, List<String> operations) {
    if (raw == null) return null;
    if (raw is! Map<String, dynamic>) return null;
    final exportOp = raw['export_op'];
    final inspectOp = raw['inspect_op'];
    final importOp = raw['import_op'];
    final importTitle = raw['import_confirm_title'];
    final importMessage = raw['import_confirm_message'];
    final kinds = raw['kinds'];
    if (exportOp is! String ||
        !operations.contains(exportOp) ||
        inspectOp is! String ||
        !operations.contains(inspectOp) ||
        importOp is! String ||
        !operations.contains(importOp) ||
        importTitle is! String ||
        importTitle.isEmpty ||
        importTitle.length > 128 ||
        importMessage is! String ||
        importMessage.isEmpty ||
        importMessage.length > 512 ||
        kinds is! List ||
        kinds.isEmpty ||
        kinds.length > 8) {
      return null;
    }
    final ids = <String>{};
    final parsedKinds = <NativeArchiveKind>[];
    for (final kind in kinds) {
      if (kind is! Map<String, dynamic>) return null;
      final id = kind['id'];
      final label = kind['label'];
      final slotMin = kind['slot_min'];
      final slotMax = kind['slot_max'];
      if (id is! String ||
          id.isEmpty ||
          !ids.add(id) ||
          label is! String ||
          label.isEmpty ||
          label.length > 64) {
        return null;
      }
      if ((slotMin == null) != (slotMax == null)) return null;
      if (slotMin != null &&
          (slotMin is! int || slotMax is! int || slotMin > slotMax)) {
        return null;
      }
      parsedKinds.add(NativeArchiveKind(
        id: id,
        label: label,
        slotMin: slotMin as int?,
        slotMax: slotMax as int?,
      ));
    }
    return NativeArchiveDescriptor(
      exportOp: exportOp,
      inspectOp: inspectOp,
      importOp: importOp,
      importConfirmTitle: importTitle,
      importConfirmMessage: importMessage,
      kinds: parsedKinds,
    );
  }
}
