class ProviderDiagnosticRow {
  final String key, value, status;
  final String? label;

  const ProviderDiagnosticRow(this.key, this.value, this.status, this.label);
}

class ProviderDiagnostics {
  final String driver, description;
  final bool supported;
  final int offset, total;
  final List<ProviderDiagnosticRow> rows;

  const ProviderDiagnostics._(
    this.driver,
    this.description,
    this.supported,
    this.offset,
    this.total,
    this.rows,
  );

  static ProviderDiagnostics? fromFrame(Map<String, dynamic> frame) {
    final data = frame['data'];
    final driver = frame['driver'];
    if (driver is! String || data is! Map) return null;
    final source = data['rows'];
    final offset = data['offset'];
    final total = data['total'];
    if (source is! List ||
        offset is! int ||
        total is! int ||
        offset < 0 ||
        total < 0 ||
        offset > total ||
        source.length > total - offset) {
      return null;
    }
    final rows = <ProviderDiagnosticRow>[];
    for (final row in source) {
      if (row is! Map ||
          row['key'] is! String ||
          row['value'] is! String ||
          row['status'] is! String ||
          (row['label'] != null && row['label'] is! String)) {
        return null;
      }
      rows.add(
        ProviderDiagnosticRow(
          row['key'] as String,
          row['value'] as String,
          row['status'] as String,
          row['label'] as String?,
        ),
      );
    }
    return ProviderDiagnostics._(
      driver,
      data['description'] as String? ?? data['reason'] as String? ?? '',
      data['supported'] == true,
      offset,
      total,
      List.unmodifiable(rows),
    );
  }
}
