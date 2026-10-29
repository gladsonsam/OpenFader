import 'package:flutter/foundation.dart';

/// Provider-neutral selection presented by the shared console header.
class SelectedControlReadoutData {
  final String target, label, value;
  final bool unconfirmed;
  final VoidCallback? onEdit;

  const SelectedControlReadoutData({
    required this.target,
    required this.label,
    required this.value,
    this.unconfirmed = false,
    this.onEdit,
  });
}
