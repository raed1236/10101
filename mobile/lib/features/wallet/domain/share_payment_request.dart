import 'package:get_10101/common/domain/model.dart';

class SharePaymentRequest {
  final String bip21Uri;
  final Amount? amount;

  SharePaymentRequest({required this.bip21Uri, required this.amount});
}
