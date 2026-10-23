// Launch (splash) screen: logo + name + live boot status.
//
// Shown on the very first frame while [BootController] works through its
// stages. Renders the step checklist (done / active / pending), a progress
// bar, and the current status line ("Loading native engine…"), plus the
// backend verdict and a Retry button when boot fails.
library;

import 'package:flutter/material.dart';

import '../theme.dart';
import 'app_logo.dart';
import 'boot_controller.dart';

class LaunchScreen extends StatelessWidget {
  final BootController controller;
  final VoidCallback? onRetry;

  const LaunchScreen({super.key, required this.controller, this.onRetry});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: U4.page,
      body: SafeArea(
        child: ListenableBuilder(
          listenable: controller,
          builder: (context, _) {
            final failed = controller.error != null;
            return Center(
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(24),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.center,
                  children: [
                    const AppLogo(),
                    const SizedBox(height: 20),
                    const Text('OpenMix',
                        style: TextStyle(
                            color: U4.text1,
                            fontSize: 30,
                            fontWeight: FontWeight.w700,
                            letterSpacing: 0.5)),
                    const SizedBox(height: 4),
                    const Text('CONSOLE REMOTE',
                        style: TextStyle(
                            color: U4.text7,
                            fontSize: 12,
                            fontWeight: FontWeight.w600,
                            letterSpacing: 3)),
                    const SizedBox(height: 28),
                    SizedBox(
                      width: 220,
                      child: failed
                          ? const LinearProgressIndicator(
                              value: 0,
                              backgroundColor: U4.control,
                              valueColor:
                                  AlwaysStoppedAnimation<Color>(U4.meterRed),
                            )
                          : LinearProgressIndicator(
                              value: controller.done
                                  ? 1
                                  : (controller.progress <= 0
                                      ? null
                                      : controller.progress),
                              backgroundColor: U4.control,
                              valueColor: const AlwaysStoppedAnimation<Color>(
                                  U4.cyan),
                            ),
                    ),
                    const SizedBox(height: 12),
                    Text(controller.status,
                        textAlign: TextAlign.center,
                        style:
                            const TextStyle(color: U4.text4, fontSize: 13)),
                    const SizedBox(height: 20),
                    _StepList(controller: controller),
                    if (controller.engineLabel != null && !failed) ...[
                      const SizedBox(height: 12),
                      Text('engine: ${controller.engineLabel}',
                          style: const TextStyle(
                              color: U4.text7, fontSize: 12)),
                    ],
                    if (failed) ...[
                      const SizedBox(height: 12),
                      Text(controller.error!,
                          textAlign: TextAlign.center,
                          style: const TextStyle(
                              color: U4.meterRed, fontSize: 12)),
                      const SizedBox(height: 12),
                      TextButton(
                        onPressed:
                            controller.running ? null : (onRetry ?? () {}),
                        child: Text(controller.running
                            ? 'Retrying…'
                            : 'Retry'),
                      ),
                    ],
                  ],
                ),
              ),
            );
          },
        ),
      ),
    );
  }
}

class _StepList extends StatelessWidget {
  final BootController controller;

  const _StepList({required this.controller});

  @override
  Widget build(BuildContext context) {
    final failed = controller.error != null;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (var i = 0; i < BootController.steps.length; i++)
          _StepRow(
            label: BootController.steps[i],
            state: controller.done || i < controller.step
                ? _StepState.done
                : (failed && i == controller.step
                    ? _StepState.failed
                    : (i == controller.step
                        ? _StepState.active
                        : _StepState.pending)),
          ),
      ],
    );
  }
}

enum _StepState { done, active, pending, failed }

class _StepRow extends StatelessWidget {
  final String label;
  final _StepState state;

  const _StepRow({required this.label, required this.state});

  @override
  Widget build(BuildContext context) {
    final Widget icon;
    final Color textColor;
    switch (state) {
      case _StepState.done:
        icon = const Icon(Icons.check_circle_outline,
            size: 16, color: U4.meterGreen);
        textColor = U4.text4;
        break;
      case _StepState.active:
        icon = const SizedBox(
          width: 16,
          height: 16,
          child: CircularProgressIndicator(
              strokeWidth: 2,
              valueColor: AlwaysStoppedAnimation<Color>(U4.cyan)),
        );
        textColor = U4.text1;
        break;
      case _StepState.pending:
        icon = const Icon(Icons.radio_button_unchecked,
            size: 16, color: U4.text10);
        textColor = U4.text7;
        break;
      case _StepState.failed:
        icon =
            const Icon(Icons.error_outline, size: 16, color: U4.meterRed);
        textColor = U4.meterRed;
        break;
    }
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 3),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          icon,
          const SizedBox(width: 8),
          Text(label, style: TextStyle(color: textColor, fontSize: 13)),
        ],
      ),
    );
  }
}
