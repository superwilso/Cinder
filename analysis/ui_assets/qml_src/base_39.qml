//
// Copyright 2018 Sony Corporation
//
import QtQuick 2.3

Item
{
  id: base
  /** type int 同期箇所のインデックス */
  property int sync_lyric_current_index
  /** type var 歌詞情報リスト */
  property var lyric_list
  /** type var ナビゲーションバー情報 */
  property var navigation_bar

  /**
   * 歌詞表示のクローズ
   */
  signal closeLyricDisp()
  /**
   * インデックスジャンプ
  * @param index インデックスジャンプを行う同期箇所のインデックス
   */
  signal indexJump(int index)
  /**
   * 歌詞表示のスクロール
   */
  signal scrolled()
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

  /**
   * 歌詞なし表示を行う
   */
  function lyricDispEmpty()
  {
    lyric_disp.lyricDispEmpty()
  }
  /**
   * 歌詞読み込み失敗表示を行う
   */
  function lyricDispInvalid()
  {
    lyric_disp.lyricDispInvalid()
  }
  /**
   * 歌詞の非表示処理を行う
   */
  function lyricDispInit()
  {
    lyric_disp.lyricDispInit()
  }
  /**
   * 歌詞を先頭位置に表示する
   */
  function lyricDispScrollPositionReset()
  {
    lyric_disp.lyricDispScrollPositionReset()
  }
  /**
   * 歌詞をスクロールなしで同期箇所を更新する
   */
  function lyricDispNotScroll()
  {
    lyric_disp.lyricDispNotScroll()
  }
  /**
   * 歌詞をアニメーションなしでスクロールを行う
   */
  function lyricDispScrollAnimationOff()
  {
    lyric_disp.lyricDispScrollAnimationOff()
  }
  /**
   * 歌詞をアニメーションありでスクロールを行う
   */
  function lyricDispScrollAnimationOn()
  {
    lyric_disp.lyricDispScrollAnimationOn()
  }

  // 歌詞表示
  SyncLyricDisp
  {
    id: lyric_disp
    anchors.fill: parent
    sync_lyric_current_index: base.sync_lyric_current_index
    lyric_list: base.lyric_list
    navigation_bar: base.navigation_bar
    popup_area_position_y: 23
    popup_area_height: 568
    list_top_margin: 18
    list_bottom_margin: 12
    text_top_margin: 14

    onCloseLyricDisp: base.closeLyricDisp()
    onIndexJump: base.indexJump(index)
    onScrolled: base.scrolled()
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
      topMargin: lyric_disp.popup_area_position_y + lyric_disp.popup_area_height;
    }
    height: 46
  }
}
