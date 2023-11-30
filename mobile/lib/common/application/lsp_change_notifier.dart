import 'package:flutter/material.dart';
import 'package:get_10101/bridge_generated/bridge_definitions.dart' as bridge;
import 'package:get_10101/common/application/channel_info_service.dart';
import 'package:get_10101/common/application/event_service.dart';
import 'package:get_10101/common/domain/liquidity_option.dart';
import 'package:get_10101/common/domain/model.dart';

class LspChangeNotifier extends ChangeNotifier implements Subscriber {
  ChannelInfoService channelInfoService;

  List<LiquidityOption> liquidityOptions = [];
  int contractTxFeeRate = 0;

  LspChangeNotifier(this.channelInfoService);

  List<LiquidityOption> getLiquidityOptions(bool activeOnly) {
    return liquidityOptions.where((option) => option.active || !activeOnly).toList();
  }

  Future<Amount> getTradeFeeReserve() async {
    double txFeesreserveForForceCloseAtOneSatsPerVbyte = 416.5;

    int satsPerVbyte = await channelInfoService.getContractTxFeeRate() ?? contractTxFeeRate;
    int feeReserve = (txFeesreserveForForceCloseAtOneSatsPerVbyte * satsPerVbyte).ceil();
    return Amount(feeReserve);
  }

  @override
  void notify(bridge.Event event) {
    if (event is bridge.Event_Authenticated) {
      liquidityOptions =
          event.field0.liquidityOptions.map((lo) => LiquidityOption.from(lo)).toList();

      liquidityOptions.sort((a, b) => a.rank.compareTo(b.rank));
      contractTxFeeRate = event.field0.contractTxFeeRate;
      super.notifyListeners();
    }
  }
}
