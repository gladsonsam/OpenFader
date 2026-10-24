// Launch screen: logo, name, live boot status, and Retry on failure.
library;

import 'package:flutter/material.dart';
import '../palette.dart';

import 'app_logo.dart';
import 'boot_controller.dart';

class LaunchScreen extends StatelessWidget {
  final BootController controller;
  final VoidCallback? onRetry;

  const LaunchScreen({super.key, required this.controller, this.onRetry});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: ConsolePalette.of(context).page,
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
                    Text('OpenFader',
                        style: TextStyle(
                            color: ConsolePalette.of(context).text1,
                            fontSize: 30,
                            fontWeight: FontWeight.w700,
                            letterSpacing: 0.5)),
                    const SizedBox(height: 4),
                    Text('CONSOLE REMOTE',
                        style: TextStyle(
                            color: ConsolePalette.of(context).text7,
                            fontSize: 12,
                            fontWeight: FontWeight.w600,
                            letterSpacing: 3)),
                    const SizedBox(height: 28),
                    SizedBox(
                      width: 220,
                      child: failed
                          ? LinearProgressIndicator(
                              value: 0,
                              backgroundColor:
                                  ConsolePalette.of(context).control,
                              valueColor: AlwaysStoppedAnimation<Color>(
                                  ConsolePalette.of(context).meterRed),
                            )
                          : LinearProgressIndicator(
                              value: controller.done
                                  ? 1
                                  : (controller.progress <= 0
                                      ? null
                                      : controller.progress),
                              backgroundColor:
                                  ConsolePalette.of(context).control,
                              valueColor: AlwaysStoppedAnimation<Color>(
                                  ConsolePalette.of(context).cyan),
                            ),
                    ),
                    const SizedBox(height: 12),
                    Text(controller.status,
                        textAlign: TextAlign.center,
                        style: TextStyle(
                            color: ConsolePalette.of(context).text4,
                            fontSize: 13)),
                    const SizedBox(height: 20),
                    _StepList(controller: controller),
                    if (controller.engineLabel != null && !failed) ...[
                      const SizedBox(height: 12),
                      Text('engine: ${controller.engineLabel}',
                          style: TextStyle(
                              color: ConsolePalette.of(context).text7,
                              fontSize: 12)),
                    ],
                    if (failed) ...[
                      const SizedBox(height: 12),
                      Text(controller.error!,
                          textAlign: TextAlign.center,
                          style: TextStyle(
                              color: ConsolePalette.of(context).meterRed,
                              fontSize: 12)),
                      const SizedBox(height: 12),
                      TextButton(
                        onPressed:
                            controller.running ? null : (onRetry ?? () {}),
                        child: Text(controller.running ? 'Retrying…' : 'Retry'),
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
        icon = Icon(Icons.check_circle_outline,
            size: 16, color: ConsolePalette.of(context).meterGreen);
        textColor = ConsolePalette.of(context).text4;
        break;
      case _StepState.active:
        icon = SizedBox(
          width: 16,
          height: 16,
          child: CircularProgressIndicator(
              strokeWidth: 2,
              valueColor: AlwaysStoppedAnimation<Color>(
                  ConsolePalette.of(context).cyan)),
        );
        textColor = ConsolePalette.of(context).text1;
        break;
      case _StepState.pending:
        icon = Icon(Icons.radio_button_unchecked,
            size: 16, color: ConsolePalette.of(context).text10);
        textColor = ConsolePalette.of(context).text7;
        break;
      case _StepState.failed:
        icon = Icon(Icons.error_outline,
            size: 16, color: ConsolePalette.of(context).meterRed);
        textColor = ConsolePalette.of(context).meterRed;
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
