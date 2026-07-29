
/* ===== ConverPlayStatus  @ 00012840 ===== */

void __thiscall
pst::services::playerservice::PlayerService::ConverPlayStatus
          (PlayerService *this,PlayStatus *param_1,PlayStatus *param_2)

{
  (*(code *)PTR_ConverPlayStatus_00015f00)(this);
  return;
}



/* ===== Connect  @ 00012864 ===== */

void __thiscall
pst::services::playerservice::PlayController::Connect
          (PlayController *this,PlayEventListener *param_1)

{
                    /* WARNING: Could not recover jumptable at 0x0001286c. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (*(code *)PTR_Connect_00015f0c)(this);
  return;
}



/* ===== OnPlayTimeUpdated  @ 00012978 ===== */

void pst::services::playerservice::PlayController::OnPlayTimeUpdated(int param_1,int param_2)

{
  (*(code *)PTR_OnPlayTimeUpdated_00015f68)(param_1);
  return;
}



/* ===== OnNextTrack  @ 000129a8 ===== */

void __thiscall pst::services::playerservice::PlayController::OnNextTrack(void)

{
  (*(code *)PTR_OnNextTrack_00015f78)();
  return;
}



/* ===== Connect  @ 00012f08 ===== */

/* pst::services::playerservice::PlayController::Connect(pst::playservice::PlayEventListener*) */

void __thiscall
pst::services::playerservice::PlayController::Connect
          (PlayController *this,PlayEventListener *param_1)

{
  int *piVar1;
  int local_1c;
  undefined4 local_18;
  int local_14;
  
  local_14 = **(int **)(DAT_00012f54 + 0x12f18);
  PlayerService::AddListener(*(PlayerService **)(this + 0x10));
  local_18 = *(undefined4 *)(this + 0x34);
  (**(code **)(**(int **)(this + 0x14) + 0x1c))(*(int **)(this + 0x14),&local_18,&local_1c);
  piVar1 = *(int **)(DAT_00012f58 + 0x12f3a);
  if (local_1c == 0) {
    *(PlayEventListener **)(this + 0x30) = param_1;
    local_1c = 0;
  }
  if (*piVar1 != local_14) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(local_1c);
  }
  return;
}



/* ===== ChangePlayState  @ 00013034 ===== */

/* pst::services::playerservice::PlayController::ChangePlayState(pst::playservice::playstate_t) */

void __thiscall
pst::services::playerservice::PlayController::ChangePlayState(PlayController *this,int param_2)

{
  undefined4 local_18;
  undefined4 local_14;
  int iStack_10;
  int local_c;
  
  local_c = **(int **)(DAT_00013078 + 0x13040);
  if (param_2 - 3U < 4) {
    local_18 = 1;
  }
  else {
    local_14 = *(undefined4 *)(this + 0x34);
    iStack_10 = param_2;
    (**(code **)(**(int **)(this + 0x14) + 0x34))(*(int **)(this + 0x14),&local_14,&local_18);
  }
  if (**(int **)(DAT_0001307c + 0x1306a) != local_c) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(local_18);
  }
  return;
}



/* ===== NextTrack  @ 00013080 ===== */

/* pst::services::playerservice::PlayController::NextTrack() */

void __thiscall pst::services::playerservice::PlayController::NextTrack(PlayController *this)

{
  undefined4 local_14;
  undefined4 local_10;
  int local_c;
  
  local_c = **(int **)(DAT_000130b8 + 0x1308e);
  local_10 = *(undefined4 *)(this + 0x34);
  (**(code **)(**(int **)(this + 0x14) + 0x38))(*(int **)(this + 0x14),&local_10,&local_14);
  if (**(int **)(DAT_000130bc + 0x130a8) != local_c) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(local_14);
  }
  return;
}



/* ===== GetCurrentStatus  @ 00013108 ===== */

/* pst::services::playerservice::PlayController::GetCurrentStatus(pst::playservice::PlayStatus&) */

void __thiscall
pst::services::playerservice::PlayController::GetCurrentStatus
          (PlayController *this,PlayStatus *param_1)

{
  int iVar1;
  int local_80;
  PlayStatus aPStack_7c [48];
  undefined4 local_4c;
  undefined4 uStack_48;
  undefined4 local_44;
  undefined4 local_24;
  int local_20;
  
  local_20 = **(int **)(DAT_00013170 + 0x1311c);
  local_4c = 0;
  uStack_48 = 0;
  local_44 = 0;
  local_24 = *(undefined4 *)(this + 0x34);
  (**(code **)(**(int **)(this + 0x14) + 0x28))(*(int **)(this + 0x14),&local_24,&local_80);
  if (local_80 == 0) {
    PlayerService::ConverPlayStatus(*(PlayerService **)(this + 0x10),aPStack_7c,param_1);
  }
  std::string::~string((string *)&local_4c);
  iVar1 = **(int **)(DAT_00013174 + 0x1315c) - local_20;
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== SeekTime  @ 00013200 ===== */

/* pst::services::playerservice::PlayController::SeekTime(pst::playservice::media_origin_t, int) */

void __thiscall
pst::services::playerservice::PlayController::SeekTime
          (PlayController *this,undefined4 param_2,undefined4 param_3)

{
  undefined4 local_1c;
  undefined4 local_18;
  undefined4 uStack_14;
  undefined4 local_10;
  int local_c;
  
  local_c = **(int **)(DAT_0001323c + 0x1320c);
  local_18 = *(undefined4 *)(this + 0x34);
  uStack_14 = param_2;
  local_10 = param_3;
  (**(code **)(**(int **)(this + 0x14) + 0x48))(*(int **)(this + 0x14),&local_18,&local_1c);
  if (**(int **)(DAT_00013240 + 0x1322c) != local_c) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(local_1c);
  }
  return;
}



/* ===== OnPlayTimeUpdated  @ 00013618 ===== */

/* pst::services::playerservice::PlayController::OnPlayTimeUpdated(int, int) */

void pst::services::playerservice::PlayController::OnPlayTimeUpdated(int param_1,int param_2)

{
  int *piVar1;
  int iVar2;
  bool bVar3;
  
  iVar2 = **(int **)(DAT_00013664 + 0x13624);
  piVar1 = *(int **)(param_1 + 0x30);
  if (piVar1 == (int *)0x0) {
    piVar1 = (int *)(**(int **)(DAT_0001366c + 0x13654) - iVar2);
    if (piVar1 == (int *)0x0) {
      return;
    }
  }
  else {
    bVar3 = **(int **)(DAT_00013668 + 0x1363c) == iVar2;
    iVar2 = param_2;
    if (bVar3) {
                    /* WARNING: Could not recover jumptable at 0x0001364a. Too many branches */
                    /* WARNING: Treating indirect jump as call */
      (**(code **)(*piVar1 + 0xc))();
      return;
    }
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail(piVar1,iVar2);
}



/* ===== OnNextTrack  @ 000138c0 ===== */

/* pst::services::playerservice::PlayController::OnNextTrack(pst::playservice::change_track_mode_t)
    */

void __thiscall
pst::services::playerservice::PlayController::OnNextTrack(PlayController *this,int param_2)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00013908 + 0x138cc);
  if (*(int **)(param_2 + 0x38) == (int *)0x0) {
    *(undefined4 *)this = 0;
    if (**(int **)(DAT_00013910 + 0x138fa) == iVar1) {
      return;
    }
  }
  else {
    (**(code **)(**(int **)(param_2 + 0x38) + 0x14))();
    if (**(int **)(DAT_0001390c + 0x138e2) == iVar1) {
      return;
    }
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail();
}



/* ===== GetInstance  @ 00013c44 ===== */

/* pst::services::playerservice::PlayerService::GetInstance() */

void pst::services::playerservice::PlayerService::GetInstance(void)

{
  int iVar1;
  PlayerService *this;
  undefined4 uVar2;
  
  iVar1 = **(int **)(DAT_00013c84 + 0x13c52);
  if (*(int *)(DAT_00013c88 + 0x13c54) == 0) {
    this = operator_new(0x24);
    uVar2 = PlayerService(this);
    *(undefined4 *)(DAT_00013c8c + 0x13c6c) = uVar2;
  }
  if (**(int **)(DAT_00013c90 + 0x13c74) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== getPlayController  @ 00013c94 ===== */

/* pst::services::playerservice::PlayerService::getPlayController(char const*) */

void pst::services::playerservice::PlayerService::getPlayController(char *param_1)

{
  int *piVar1;
  undefined4 *puVar2;
  PlayerService *in_r1;
  char *in_r2;
  PlayController *this;
  PlayerService *pPVar3;
  PlayController *local_2c;
  int *local_28;
  int local_24;
  
  local_24 = **(int **)(DAT_00013d4c + 0x13ca8);
  if (((byte)in_r1[0x18] & 1) == 0) {
    pPVar3 = in_r1 + 0x19;
  }
  else {
    pPVar3 = *(PlayerService **)(in_r1 + 0x20);
  }
  piVar1 = operator_new(0x4c);
  *piVar1 = *(int *)(DAT_00013d50 + 0x13cd4) + 8;
  piVar1[1] = 0;
  piVar1[2] = 0;
  this = (PlayController *)(piVar1 + 3);
  PlayController::PlayController(this,in_r1,*(IPlayerService **)(in_r1 + 4),(char *)pPVar3,in_r2);
  local_2c = this;
  local_28 = piVar1;
  std::__shared_weak_count::__add_weak();
  puVar2 = *(undefined4 **)(in_r1 + 0xc);
  if (puVar2 < *(undefined4 **)(in_r1 + 0x10)) {
    *puVar2 = this;
    puVar2[1] = piVar1;
    local_2c = (PlayController *)0x0;
    local_28 = (int *)0x0;
    *(undefined4 **)(in_r1 + 0xc) = puVar2 + 2;
  }
  else {
    FUN_00014658(in_r1 + 8,&local_2c);
    if (local_28 != (int *)0x0) {
      std::__shared_weak_count::__release_weak();
    }
  }
  *(PlayController **)param_1 = this;
  *(int **)(param_1 + 4) = piVar1;
  std::__shared_weak_count::__add_shared();
  std::__shared_weak_count::__release_shared();
  if (**(int **)(DAT_00013d54 + 0x13d3a) != local_24) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== ConverPlayStatus  @ 00013fc8 ===== */

/* pst::services::playerservice::PlayerService::ConverPlayStatus(IPlayerService::PlayStatus const&,
   pst::playservice::PlayStatus&) */

void __thiscall
pst::services::playerservice::PlayerService::ConverPlayStatus
          (PlayerService *this,PlayStatus *param_1,PlayStatus *param_2)

{
  int iVar1;
  undefined8 uVar2;
  
  iVar1 = **(int **)(DAT_00014054 + 0x13fdc);
  *(undefined4 *)param_2 = *(undefined4 *)param_1;
  *(undefined4 *)(param_2 + 8) = *(undefined4 *)(param_1 + 4);
  *(undefined4 *)(param_2 + 0xc) = *(undefined4 *)(param_1 + 8);
  *(undefined4 *)(param_2 + 0x14) = *(undefined4 *)(param_1 + 0xc);
  *(undefined4 *)(param_2 + 0x18) = *(undefined4 *)(param_1 + 0x10);
  *(undefined4 *)(param_2 + 0x30) = *(undefined4 *)(param_1 + 0x14);
  *(undefined4 *)(param_2 + 0x38) = *(undefined4 *)(param_1 + 0x18);
  *(undefined4 *)(param_2 + 0x3c) = *(undefined4 *)(param_1 + 0x1c);
  uVar2 = *(undefined8 *)(param_1 + 0x28);
  *(undefined8 *)(param_2 + 0x44) = *(undefined8 *)(param_1 + 0x20);
  *(undefined8 *)(param_2 + 0x4c) = uVar2;
  std::string::operator=((string *)(param_2 + 0x6c),(string *)(param_1 + 0x30));
  uVar2 = *(undefined8 *)(param_1 + 0x44);
  *(undefined8 *)(param_2 + 0x54) = *(undefined8 *)(param_1 + 0x3c);
  *(undefined8 *)(param_2 + 0x5c) = uVar2;
  *(undefined4 *)(param_2 + 100) = *(undefined4 *)(param_1 + 0x4c);
  *(undefined4 *)(param_2 + 0x68) = *(undefined4 *)(param_1 + 0x50);
  param_2[0x78] = (PlayStatus)(*(int *)(param_1 + 0x54) != 0);
  if (**(int **)(DAT_00014058 + 0x14044) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== PlayController_OnPlayTimeUpdated  @ 00014130 ===== */

/* pst::services::playerservice::PlayerService::PlayController_OnPlayTimeUpdated(IPlayerService::PlayController_OnPlayTimeUpdated_Param
   const&) */

void __thiscall
pst::services::playerservice::PlayerService::PlayController_OnPlayTimeUpdated
          (PlayerService *this,PlayController_OnPlayTimeUpdated_Param *param_1)

{
  int local_1c;
  int local_18;
  int local_14;
  
  local_14 = **(int **)(DAT_00014184 + 0x14140);
  GetController((char *)&local_1c);
  if (local_1c != 0) {
    PlayController::OnPlayTimeUpdated(local_1c,*(int *)(param_1 + 0xc));
  }
  if (local_18 != 0) {
    std::__shared_weak_count::__release_shared();
  }
  if (**(int **)(DAT_00014188 + 0x14176) != local_14) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== PlayController_OnNextTrack  @ 000143c0 ===== */

/* pst::services::playerservice::PlayerService::PlayController_OnNextTrack(IPlayerService::PlayController_TrackSequence_OnTrack_Request
   const&, IPlayerService::PlayController_TrackSequence_Param&) */

void __thiscall
pst::services::playerservice::PlayerService::PlayController_OnNextTrack
          (PlayerService *this,PlayController_TrackSequence_OnTrack_Request *param_1,
          PlayController_TrackSequence_Param *param_2)

{
  undefined4 *puVar1;
  undefined4 *local_28;
  int local_24;
  int local_20;
  int local_1c;
  
  local_1c = **(int **)(DAT_00014478 + 0x143d4);
  GetController((char *)&local_24);
  if (local_24 == 0) {
    *(undefined4 *)param_2 = 0;
  }
  else {
    PlayController::OnNextTrack((PlayController *)&local_28,local_24,*(undefined4 *)(param_1 + 0xc))
    ;
    puVar1 = local_28;
    local_28 = (void *)0x0;
    if (puVar1 == (undefined4 *)0x0) {
      *(undefined4 *)param_2 = 0;
      local_28 = (undefined4 *)0x0;
    }
    else {
      *(undefined4 *)param_2 = 1;
      *(undefined4 *)(param_2 + 4) = *puVar1;
      *(undefined4 *)(param_2 + 8) = puVar1[1];
      std::string::operator=((string *)(param_2 + 0xc),(string *)(puVar1 + 2));
      *(uint *)(param_2 + 0x18) = (uint)*(byte *)(puVar1 + 5);
      std::string::~string((string *)(puVar1 + 2));
      operator_delete(puVar1);
      puVar1 = local_28;
      local_28 = (undefined4 *)0x0;
      if (puVar1 != (void *)0x0) {
        std::string::~string((string *)((int)puVar1 + 8));
        operator_delete(puVar1);
      }
    }
  }
  if (local_20 != 0) {
    std::__shared_weak_count::__release_shared();
  }
  if (**(int **)(DAT_0001447c + 0x14466) != local_1c) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}


