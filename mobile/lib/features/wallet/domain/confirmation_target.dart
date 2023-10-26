import 'package:get_10101/ffi.dart' as rust;

enum ConfirmationTarget {
  background,
  normal,
  highPriority;

  static ConfirmationTarget fromAPI(rust.ConfirmationTarget target) {
    return switch (target) {
      rust.ConfirmationTarget.Background => background,
      rust.ConfirmationTarget.Normal => normal,
      rust.ConfirmationTarget.HighPriority => highPriority,
    };
  }

  @override
  String toString() {
    return switch (this) {
      background => "Background",
      normal => "Normal",
      highPriority => "High Priority",
    };
  }

  rust.ConfirmationTarget toAPI() {
    return switch (this) {
      background => rust.ConfirmationTarget.Background,
      normal => rust.ConfirmationTarget.Normal,
      highPriority => rust.ConfirmationTarget.HighPriority,
    };
  }
}
