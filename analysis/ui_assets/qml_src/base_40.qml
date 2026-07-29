//
// Copyright 2018 Sony Corporation
//

import QtQuick 2.3

Item
{
  id: base
  /** type var 詳細情報リスト */
  property var contentDetailedInfoList
  /** type var ナビゲーションバー情報 */
  property var navigation

  /**
   * 詳細情報表示のクローズ
   */
  signal closeDetailedInfoPopup()
  /**
   * NavigationBar Backボタンタップ
   */
  signal backButtonClicked()
  /**
   * NavigationBar 機能トップボタンタップ
   */
  signal libraryTopButtonClicked()
  /**
   * NavigationBar OptionMenuボタンタップ
  * @param index オプションメニューのインデックス
   */
  signal optionMenuButtonClicked(int index)

  // 詳細情報表示
  ContentDetailedInfo
  {
    id: detail_info
    anchors.fill: parent
    contentDetailedInfoList: base.contentDetailedInfoList
    navigation: base.navigation
    popup_area_position_y: 23
    popup_area_height: 568

    onCloseDetailedInfoPopup: base.closeDetailedInfoPopup()
    onBackButtonClicked: base.backButtonClicked()
    onLibraryTopButtonClicked: base.libraryTopButtonClicked()
    onOptionMenuButtonClicked: base.optionMenuButtonClicked(index)
  }
  // ポップアップ枠下部に位置するシークバー操作領域を覆い隠すことで、シークバー操作を受け付けないようにする
  MouseArea
  {
    anchors
    {
      top: parent.top;
      left: parent.left;
      right: parent.right;
      topMargin: detail_info.popup_area_position_y + detail_info.popup_area_height;
    }
    height: 46
  }
}
