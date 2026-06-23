
/* ===== _init  @ 00013814 ===== */

int _init(EVP_PKEY_CTX *ctx)

{
  int iVar1;
  
  iVar1 = FUN_00013d5c();
  return iVar1;
}



/* ===== __cxa_finalize  @ 00013840 ===== */

void __cxa_finalize(void)

{
  (*(code *)PTR___cxa_finalize_00019e4c)();
  return;
}



/* ===== GetReference  @ 0001384c ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void pst::core::Framework::GetReference(void)

{
  (*(code *)PTR_GetReference_00019e50)();
  return;
}



/* ===== operator.new  @ 00013858 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void * operator_new(uint param_1)

{
  void *pvVar1;
  
  pvVar1 = (void *)(*(code *)PTR_operator_new_00019e54)(param_1);
  return pvVar1;
}



/* ===== LifeCycleManager  @ 00013864 ===== */

void __thiscall easel::LifeCycleManager::LifeCycleManager(LifeCycleManager *this)

{
  (*(code *)PTR_LifeCycleManager_00019e58)(this);
  return;
}



/* ===== __stack_chk_fail  @ 00013870 ===== */

void __stack_chk_fail(void)

{
  (*(code *)PTR___stack_chk_fail_00019e5c)();
  return;
}



/* ===== GetModuleRegistry  @ 0001387c ===== */

void __thiscall easel::LifeCycleManager::GetModuleRegistry(LifeCycleManager *this)

{
  (*(code *)PTR_GetModuleRegistry_00019e60)(this);
  return;
}



/* ===== DoGetModule  @ 00013888 ===== */

void __thiscall easel::ModuleRegistry::DoGetModule(ModuleRegistry *this,uint param_1)

{
  (*(code *)PTR_DoGetModule_00019e64)(this);
  return;
}



/* ===== GetAppParam  @ 00013894 ===== */

void __thiscall easel::AppManagerModule::GetAppParam(AppManagerModule *this)

{
  (*(code *)PTR_GetAppParam_00019e68)(this);
  return;
}



/* ===== SetPumpTriggerHandler  @ 000138a0 ===== */

void __thiscall easel::LifeCycleManager::SetPumpTriggerHandler(void)

{
  (*(code *)PTR_SetPumpTriggerHandler_00019e6c)();
  return;
}



/* ===== operator.delete  @ 000138ac ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void operator_delete(void *param_1)

{
  (*(code *)PTR_operator_delete_00019e70)(param_1);
  return;
}



/* ===== Inactivate  @ 000138b8 ===== */

void __thiscall easel::LifeCycleManager::Inactivate(LifeCycleManager *this)

{
                    /* WARNING: Could not recover jumptable at 0x000138c0. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (*(code *)PTR_Inactivate_00019e74)(this);
  return;
}



/* ===== GetAppManager  @ 000138c4 ===== */

void easel::AppManagerModule::GetAppManager(void)

{
  (*(code *)PTR_GetAppManager_00019e78)();
  return;
}



/* ===== lock  @ 000138d0 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void std::mutex::lock(void)

{
  (*(code *)PTR_lock_00019e7c)();
  return;
}



/* ===== StopTimeoutCheck  @ 000138dc ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void pst::appmanager::AppManager::Counter::StopTimeoutCheck(void)

{
  (*(code *)PTR_StopTimeoutCheck_00019e80)();
  return;
}



/* ===== unlock  @ 000138e8 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void std::mutex::unlock(void)

{
  (*(code *)PTR_unlock_00019e84)();
  return;
}



/* ===== ParseResumeFactorString  @ 000138f4 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void pst::services::binder::ServiceBase::ParseResumeFactorString
               (string *param_1,string *param_2,string *param_3)

{
  (*(code *)PTR_ParseResumeFactorString_00019e88)(param_1);
  return;
}



/* ===== AppManagerModule  @ 00013900 ===== */

void __thiscall
easel::AppManagerModule::AppManagerModule
          (AppManagerModule *this,int param_1,char **param_2,char *param_3,
          ChangeLifeCycleHandler *param_4)

{
  (*(code *)PTR_AppManagerModule_00019e8c)(this);
  return;
}



/* ===== SetPowerStateHandler  @ 0001390c ===== */

void __thiscall pst::appmanager::AppManager::SetPowerStateHandler(void)

{
  (*(code *)PTR_SetPowerStateHandler_00019e90)();
  return;
}



/* ===== SetResetSettingHandler  @ 00013918 ===== */

void __thiscall pst::appmanager::AppManager::SetResetSettingHandler(void)

{
  (*(code *)PTR_SetResetSettingHandler_00019e94)();
  return;
}



/* ===== ModuleRegistry  @ 00013924 ===== */

void __thiscall easel::ModuleRegistry::ModuleRegistry(ModuleRegistry *this)

{
  (*(code *)PTR_ModuleRegistry_00019e98)(this);
  return;
}



/* ===== Register  @ 00013930 ===== */

void __thiscall easel::ModuleRegistry::Register(void)

{
  (*(code *)PTR_Register_00019e9c)();
  return;
}



/* ===== Main  @ 0001393c ===== */

void __thiscall easel::LifeCycleManager::Main(void)

{
  (*(code *)PTR_Main_00019ea0)();
  return;
}



/* ===== swap  @ 00013948 ===== */

void __thiscall std::function<void(bool&)>::swap(function<void(bool&)> *this,function *param_1)

{
  (*(code *)PTR_swap_00019ea4)(this);
  return;
}



/* ===== swap  @ 00013954 ===== */

void __thiscall
std::function<void(std::string_const&)>::swap
          (function<void(std::string_const&)> *this,function *param_1)

{
  (*(code *)PTR_swap_00019ea8)(this);
  return;
}



/* ===== swap  @ 00013960 ===== */

void __thiscall std::function<void()>::swap(function<void()> *this,function *param_1)

{
  (*(code *)PTR_swap_00019eac)(this);
  return;
}



/* ===== Print  @ 0001396c ===== */

void pst::log::Print(void...)

{
  (*(code *)PTR_Print_00019eb0)();
  return;
}



/* ===== fork  @ 00013978 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

__pid_t fork(void)

{
  __pid_t _Var1;
  
  _Var1 = (*(code *)PTR_fork_00019eb4)();
  return _Var1;
}



/* ===== waitpid  @ 00013984 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

__pid_t waitpid(__pid_t __pid,int *__stat_loc,int __options)

{
  __pid_t _Var1;
  
  _Var1 = (*(code *)PTR_waitpid_00019eb8)(__pid);
  return _Var1;
}



/* ===== execlp  @ 00013990 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

int execlp(char *__file,char *__arg,...)

{
  int iVar1;
  
  iVar1 = (*(code *)PTR_execlp_00019ebc)(__file);
  return iVar1;
}



/* ===== _exit  @ 0001399c ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void _exit(int __status)

{
  (*(code *)PTR__exit_00019ec0)(__status);
  return;
}



/* ===== execve  @ 000139a8 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

int execve(char *__path,char **__argv,char **__envp)

{
  int iVar1;
  
  iVar1 = (*(code *)PTR_execve_00019ec4)(__path);
  return iVar1;
}



/* ===== UpdateCurrentContextHangTimeout  @ 000139b4 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void pst::core::Framework::UpdateCurrentContextHangTimeout(ulonglong param_1)

{
  (*(code *)PTR_UpdateCurrentContextHangTimeout_00019ec8)((int)param_1,(int)(param_1 >> 0x20));
  return;
}



/* ===== __cxa_pure_virtual  @ 000139c0 ===== */

void __cxa_pure_virtual(void)

{
  (*(code *)PTR___cxa_pure_virtual_00019ecc)();
  return;
}



/* ===== StartForApplication  @ 00013aa4 ===== */

void pst::core::Framework::StartForApplication(void)

{
  (*(code *)PTR_StartForApplication_00019f18)();
  return;
}



/* ===== StopForApplication  @ 00013ab0 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void pst::core::Framework::StopForApplication(void)

{
  (*(code *)PTR_StopForApplication_00019f1c)();
  return;
}



/* ===== Framework  @ 00013abc ===== */

void __thiscall easel::Framework::Framework(void)

{
  (*(code *)PTR_Framework_00019f20)();
  return;
}



/* ===== ~Framework  @ 00013ac8 ===== */

void __thiscall easel::Framework::~Framework(Framework *this)

{
  (*(code *)PTR__Framework_00019f24)(this);
  return;
}



/* ===== AddJobFunc  @ 00013ad4 ===== */

void pst::core::JobQueue::AddJobFunc(void)

{
  (*(code *)PTR_AddJobFunc_00019f28)();
  return;
}



/* ===== __release_shared  @ 00013ae0 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void std::__shared_weak_count::__release_shared(void)

{
  (*(code *)PTR___release_shared_00019f2c)();
  return;
}



/* ===== FuncJob  @ 00013aec ===== */

void __thiscall pst::core::FuncJob::FuncJob(void)

{
  (*(code *)PTR_FuncJob_00019f30)();
  return;
}



/* ===== ~__shared_weak_count  @ 00013af8 ===== */

void __thiscall std::__shared_weak_count::~__shared_weak_count(__shared_weak_count *this)

{
  (*(code *)PTR____shared_weak_count_00019f34)(this);
  return;
}



/* ===== ~LifeCycleManager  @ 00013b04 ===== */

void __thiscall easel::LifeCycleManager::~LifeCycleManager(LifeCycleManager *this)

{
                    /* WARNING: Could not recover jumptable at 0x00013b0c. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (*(code *)PTR__LifeCycleManager_00019f38)(this);
  return;
}



/* ===== ~LifeCycleManager  @ 00013b10 ===== */

void __thiscall easel::LifeCycleManager::~LifeCycleManager(LifeCycleManager *this)

{
                    /* WARNING: Could not recover jumptable at 0x00013b18. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (*(code *)PTR__LifeCycleManager_00019f3c)(this);
  return;
}



/* ===== __cxa_guard_acquire  @ 00013bb8 ===== */

void __cxa_guard_acquire(void)

{
  (*(code *)PTR___cxa_guard_acquire_00019f74)();
  return;
}



/* ===== AppManager  @ 00013bc4 ===== */

void __thiscall pst::appmanager::AppManager::AppManager(AppManager *this)

{
  (*(code *)PTR_AppManager_00019f78)(this);
  return;
}



/* ===== __cxa_atexit  @ 00013bd0 ===== */

void __cxa_atexit(void)

{
  (*(code *)PTR___cxa_atexit_00019f7c)();
  return;
}



/* ===== __cxa_guard_release  @ 00013bdc ===== */

void __cxa_guard_release(void)

{
  (*(code *)PTR___cxa_guard_release_00019f80)();
  return;
}



/* ===== StartApp  @ 00013be8 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void pst::appmanager::AppManager::StartApp(string *param_1)

{
  (*(code *)PTR_StartApp_00019f84)(param_1);
  return;
}



/* ===== string  @ 00013bf4 ===== */

void __thiscall std::string::string(string *this,string *param_1)

{
  (*(code *)PTR_string_00019f88)(this);
  return;
}



/* ===== DoStartAppWithParam  @ 00013c00 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void pst::appmanager::AppManager::DoStartAppWithParam(string *param_1,string *param_2)

{
  (*(code *)PTR_DoStartAppWithParam_00019f8c)(param_1);
  return;
}



/* ===== ~string  @ 00013c0c ===== */

void __thiscall std::string::~string(string *this)

{
  (*(code *)PTR__string_00019f90)(this);
  return;
}



/* ===== strlen  @ 00013c18 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

size_t strlen(char *__s)

{
  size_t sVar1;
  
  sVar1 = (*(code *)PTR_strlen_00019f94)(__s);
  return sVar1;
}



/* ===== __init  @ 00013c24 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void std::string::__init(char *param_1,uint param_2)

{
  (*(code *)PTR___init_00019f98)(param_1);
  return;
}



/* ===== push_back  @ 00013c30 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void std::string::push_back(char param_1)

{
  (*(code *)PTR_push_back_00019f9c)((int)param_1);
  return;
}



/* ===== append  @ 00013c3c ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void std::string::append(char *param_1)

{
  (*(code *)PTR_append_00019fa0)(param_1);
  return;
}



/* ===== TerminateApp  @ 00013c48 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void pst::appmanager::AppManager::TerminateApp(string *param_1)

{
  (*(code *)PTR_TerminateApp_00019fa4)(param_1);
  return;
}



/* ===== Initialize  @ 00013c54 ===== */

void pst::appmanager::AppManager::Initialize(void)

{
  (*(code *)PTR_Initialize_00019fa8)();
  return;
}



/* ===== reserve  @ 00013ce4 ===== */

void __thiscall
std::vector<easel::ModuleRegistry::Entry,std::allocator<easel::ModuleRegistry::Entry>>::reserve
          (vector<easel::ModuleRegistry::Entry,std::allocator<easel::ModuleRegistry::Entry>> *this,
          uint param_1)

{
  (*(code *)PTR_reserve_00019fd8)(this);
  return;
}



/* ===== memcpy  @ 00013cf0 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void * memcpy(void *__dest,void *__src,size_t __n)

{
  void *pvVar1;
  
  pvVar1 = (void *)(*(code *)PTR_memcpy_00019fdc)(__dest);
  return pvVar1;
}



/* ===== logic_error  @ 00013cfc ===== */

void __thiscall std::logic_error::logic_error(logic_error *this,char *param_1)

{
  (*(code *)PTR_logic_error_00019fe0)(this);
  return;
}



/* ===== what  @ 00013d08 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void std::logic_error::what(void)

{
  (*(code *)PTR_what_00019fe4)();
  return;
}



/* ===== fprintf  @ 00013d14 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

int fprintf(FILE *__stream,char *__format,...)

{
  int iVar1;
  
  iVar1 = (*(code *)PTR_fprintf_00019fe8)(__stream);
  return iVar1;
}



/* ===== abort  @ 00013d20 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void abort(void)

{
  (*(code *)PTR_abort_00019fec)();
  return;
}



/* ===== __push_back_slow_path<easel::ModuleRegistry::Entry>  @ 00013d2c ===== */

void __thiscall
std::vector<easel::ModuleRegistry::Entry,std::allocator<easel::ModuleRegistry::Entry>>::
__push_back_slow_path<easel::ModuleRegistry::Entry>
          (vector<easel::ModuleRegistry::Entry,std::allocator<easel::ModuleRegistry::Entry>> *this,
          Entry *param_1)

{
  (*(code *)PTR___push_back_slow_path<easel::ModuleRegistry::Entry>_00019ff0)(this);
  return;
}



/* ===== __throw_length_error  @ 00013d38 ===== */

/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void std::__vector_base_common<true>::__throw_length_error(void)

{
  (*(code *)PTR___throw_length_error_00019ff4)();
  return;
}



/* ===== ~ModuleRegistry  @ 00013d44 ===== */

void __thiscall easel::ModuleRegistry::~ModuleRegistry(ModuleRegistry *this)

{
                    /* WARNING: Could not recover jumptable at 0x00013d4c. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (*(code *)PTR__ModuleRegistry_00019ff8)(this);
  return;
}



/* ===== ~ModuleRegistry  @ 00013d50 ===== */

void __thiscall easel::ModuleRegistry::~ModuleRegistry(ModuleRegistry *this)

{
                    /* WARNING: Could not recover jumptable at 0x00013d58. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (*(code *)PTR__ModuleRegistry_00019ffc)(this);
  return;
}



/* ===== FUN_00013d5c  @ 00013d5c ===== */

void FUN_00013d5c(void)

{
  if (*(int *)(DAT_00013d78 + 0x13d6c + DAT_00013d7c) == 0) {
    return;
  }
  (*(code *)PTR___gmon_start___00019e48)();
  return;
}



/* ===== FUN_00013d80  @ 00013d80 ===== */

void FUN_00013d80(void)

{
  if ((6 < (uint)((DAT_00013d9c + 0x13d8b) - (DAT_00013da0 + 0x13d8a))) &&
     (*(code **)(DAT_00013da4 + 0x13d96) != (code *)0x0)) {
    (**(code **)(DAT_00013da4 + 0x13d96))();
    return;
  }
  return;
}



/* ===== _FINI_0  @ 00013dd4 ===== */

void _FINI_0(void)

{
  if (*(char *)(DAT_00013e00 + 0x13ddc) == '\0') {
    if (*(int *)(DAT_00013e04 + 0x13de4) != 0) {
      __cxa_finalize(*(undefined4 *)(DAT_00013e08 + 0x13dec));
    }
    FUN_00013d80();
    *(undefined1 *)(DAT_00013e0c + 0x13dfc) = 1;
  }
  return;
}



/* ===== _INIT_0  @ 00013e10 ===== */

void _INIT_0(void)

{
  int iVar1;
  
  if ((*(int *)(DAT_00013e30 + 0x13e18) != 0) && (*(code **)(DAT_00013e34 + 0x13e26) != (code *)0x0)
     ) {
    (**(code **)(DAT_00013e34 + 0x13e26))();
  }
  iVar1 = (DAT_00013dc8 + 0x13db0) - (DAT_00013dcc + 0x13db2);
  if (((iVar1 >> 2) - (iVar1 >> 0x1f) >> 1 != 0) &&
     (*(code **)(DAT_00013dd0 + 0x13dc2) != (code *)0x0)) {
    (**(code **)(DAT_00013dd0 + 0x13dc2))();
    return;
  }
  return;
}



/* ===== ApplicationBase  @ 00013e38 ===== */

/* easel::ApplicationBase::ApplicationBase() */

void __thiscall easel::ApplicationBase::ApplicationBase(ApplicationBase *this)

{
  int iVar1;
  LifeCycleManager *this_00;
  int *piVar2;
  
  iVar1 = **(int **)(DAT_00013e90 + 0x13e48);
  *(int *)this = *(int *)(DAT_00013e94 + 0x13e4a) + 8;
  *(undefined4 *)(this + 4) = 0;
  pst::core::Framework::GetReference();
  this_00 = operator_new(0x28);
  LifeCycleManager::LifeCycleManager(this_00);
  piVar2 = *(int **)(this + 4);
  *(LifeCycleManager **)(this + 4) = this_00;
  if (piVar2 != (int *)0x0) {
    (**(code **)(*piVar2 + 4))();
  }
  iVar1 = **(int **)(DAT_00013e98 + 0x13e80) - iVar1;
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== GetAppParam  @ 00013e9c ===== */

/* easel::ApplicationBase::GetAppParam() */

void __thiscall easel::ApplicationBase::GetAppParam(ApplicationBase *this)

{
  ModuleRegistry *this_00;
  AppManagerModule *this_01;
  int iVar1;
  
  iVar1 = **(int **)(DAT_00013ed8 + 0x13ea8);
  this_00 = (ModuleRegistry *)LifeCycleManager::GetModuleRegistry(*(LifeCycleManager **)(this + 4));
  this_01 = (AppManagerModule *)
            ModuleRegistry::DoGetModule(this_00,*(uint *)(DAT_00013edc + 0x13eb8));
  AppManagerModule::GetAppParam(this_01);
  if (**(int **)(DAT_00013ee0 + 0x13ec8) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== GetAppManagerModule  @ 00013ee4 ===== */

/* easel::ApplicationBase::GetAppManagerModule() */

void __thiscall easel::ApplicationBase::GetAppManagerModule(ApplicationBase *this)

{
  undefined4 uVar1;
  int iVar2;
  
  iVar2 = **(int **)(DAT_00013f1c + 0x13ef0);
  uVar1 = LifeCycleManager::GetModuleRegistry(*(LifeCycleManager **)(this + 4));
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00013f24 + 0x13f02) == iVar2) {
                    /* WARNING: Could not recover jumptable at 0x00017774. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00017778 + 0x1777c))(uVar1,*(undefined4 *)(DAT_00013f20 + 0x13f12));
    return;
  }
  __stack_chk_fail();
}



/* ===== SetPumpTriggerHandler  @ 00013f28 ===== */

/* easel::ApplicationBase::SetPumpTriggerHandler(std::function<void ()>) */

void __thiscall easel::ApplicationBase::SetPumpTriggerHandler(ApplicationBase *this,int *param_2)

{
  int *piVar1;
  code *pcVar2;
  LifeCycleManager *pLVar3;
  int aiStack_30 [4];
  int *local_20;
  int local_14;
  
  local_20 = aiStack_30;
  local_14 = **(int **)(DAT_00013f90 + 0x13f36);
  piVar1 = (int *)param_2[4];
  pLVar3 = *(LifeCycleManager **)(this + 4);
  if (piVar1 == (int *)0x0) {
    local_20 = (int *)0x0;
  }
  else if (piVar1 == param_2) {
    (**(code **)(*piVar1 + 0xc))();
  }
  else {
    local_20 = (int *)(**(code **)(*piVar1 + 8))();
  }
  LifeCycleManager::SetPumpTriggerHandler(pLVar3,aiStack_30);
  if (local_20 == aiStack_30) {
    pcVar2 = *(code **)(*local_20 + 0x10);
  }
  else {
    if (local_20 == (int *)0x0) goto LAB_00013f7a;
    pcVar2 = *(code **)(*local_20 + 0x14);
  }
  (*pcVar2)();
LAB_00013f7a:
  if (**(int **)(DAT_00013f94 + 0x13f82) == local_14) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail();
}



/* ===== ~ApplicationBase  @ 00013f98 ===== */

/* easel::ApplicationBase::~ApplicationBase() */

void __thiscall easel::ApplicationBase::~ApplicationBase(ApplicationBase *this)

{
  int iVar1;
  int *piVar2;
  
  iVar1 = **(int **)(DAT_00013fe0 + 0x13fa8);
  *(int *)this = *(int *)(DAT_00013fe4 + 0x13faa) + 8;
  piVar2 = *(int **)(this + 4);
  *(undefined4 *)(this + 4) = 0;
  if (piVar2 != (int *)0x0) {
    (**(code **)(*piVar2 + 4))();
  }
  iVar1 = **(int **)(DAT_00013fe8 + 0x13fce) - iVar1;
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== ~ApplicationBase  @ 00013fec ===== */

/* easel::ApplicationBase::~ApplicationBase() */

void __thiscall easel::ApplicationBase::~ApplicationBase(ApplicationBase *this)

{
  int iVar1;
  int *piVar2;
  
  iVar1 = **(int **)(DAT_00014038 + 0x13ffc);
  *(int *)this = *(int *)(DAT_0001403c + 0x13ffe) + 8;
  piVar2 = *(int **)(this + 4);
  *(undefined4 *)(this + 4) = 0;
  if (piVar2 != (int *)0x0) {
    (**(code **)(*piVar2 + 4))();
  }
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00014040 + 0x14022) != iVar1) {
    __stack_chk_fail();
  }
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (*(code *)(DAT_00017788 + 0x1778c))(this);
  return;
}



/* ===== OnInitialize  @ 00014044 ===== */

/* easel::ApplicationBase::OnInitialize() */

void easel::ApplicationBase::OnInitialize(void)

{
  if (**(int **)(DAT_00014070 + 0x1405c) != **(int **)(DAT_0001406c + 82000)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== OnPostInitialize  @ 00014074 ===== */

/* easel::ApplicationBase::OnPostInitialize() */

void easel::ApplicationBase::OnPostInitialize(void)

{
  if (**(int **)(DAT_000140a0 + 0x1408c) != **(int **)(DAT_0001409c + 0x14080)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== OnActivate  @ 000140a4 ===== */

/* easel::ApplicationBase::OnActivate() */

void easel::ApplicationBase::OnActivate(void)

{
  if (**(int **)(DAT_000140d0 + 0x140bc) != **(int **)(DAT_000140cc + 0x140b0)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== OnForeground  @ 000140d4 ===== */

/* easel::ApplicationBase::OnForeground() */

void easel::ApplicationBase::OnForeground(void)

{
  if (**(int **)(DAT_00014100 + 0x140ec) != **(int **)(DAT_000140fc + 0x140e0)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== OnBackground  @ 00014104 ===== */

/* easel::ApplicationBase::OnBackground() */

void easel::ApplicationBase::OnBackground(void)

{
  if (**(int **)(DAT_00014130 + 0x1411c) != **(int **)(DAT_0001412c + 0x14110)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== OnInactivate  @ 00014134 ===== */

/* easel::ApplicationBase::OnInactivate() */

void easel::ApplicationBase::OnInactivate(void)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00014160 + 0x1414c) - **(int **)(DAT_0001415c + 0x14140);
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== OnFinalize  @ 00014164 ===== */

/* easel::ApplicationBase::OnFinalize() */

void easel::ApplicationBase::OnFinalize(void)

{
  if (**(int **)(DAT_00014190 + 0x1417c) != **(int **)(DAT_0001418c + 0x14170)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== Exit  @ 00014194 ===== */

/* easel::ApplicationBase::Exit() */

void __thiscall easel::ApplicationBase::Exit(ApplicationBase *this)

{
  ModuleRegistry *this_00;
  int *piVar1;
  int iVar2;
  
  iVar2 = **(int **)(DAT_000141d4 + 0x141a0);
  this_00 = (ModuleRegistry *)LifeCycleManager::GetModuleRegistry(*(LifeCycleManager **)(this + 4));
  piVar1 = (int *)ModuleRegistry::DoGetModule(this_00,*(uint *)(DAT_000141d8 + 0x141b0));
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_000141dc + 0x141bc) != iVar2) {
    __stack_chk_fail();
  }
                    /* WARNING: Could not recover jumptable at 0x000141d0. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (**(code **)(*piVar1 + 0x28))();
  return;
}



/* ===== Inactivate  @ 000141e0 ===== */

/* easel::ApplicationBase::Inactivate() */

void __thiscall easel::ApplicationBase::Inactivate(ApplicationBase *this)

{
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00014214 + 0x141f8) == **(int **)(DAT_00014210 + 0x141ec)) {
                    /* WARNING: Could not recover jumptable at 0x00017794. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00017798 + 0x1779c))(*(undefined4 *)(this + 4));
    return;
  }
  __stack_chk_fail();
}



/* ===== OnSuspend  @ 00014218 ===== */

/* easel::ApplicationBase::OnSuspend(bool&) */

void __thiscall easel::ApplicationBase::OnSuspend(ApplicationBase *this,bool *param_1)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00014244 + 0x14224);
  *param_1 = true;
  if (**(int **)(DAT_00014248 + 0x14234) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== OnResume  @ 0001424c ===== */

/* easel::ApplicationBase::OnResume(std::string const&) */

void easel::ApplicationBase::OnResume(string *param_1)

{
  if (**(int **)(DAT_00014278 + 0x14264) != **(int **)(DAT_00014274 + 0x14258)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== OnEarlySuspend  @ 0001427c ===== */

/* easel::ApplicationBase::OnEarlySuspend(bool&) */

void __thiscall easel::ApplicationBase::OnEarlySuspend(ApplicationBase *this,bool *param_1)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_000142a8 + 0x14288);
  *param_1 = true;
  if (**(int **)(DAT_000142ac + 0x14298) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== OnLateResume  @ 000142b0 ===== */

/* easel::ApplicationBase::OnLateResume(std::string const&) */

void easel::ApplicationBase::OnLateResume(string *param_1)

{
  if (**(int **)(DAT_000142dc + 0x142c8) != **(int **)(DAT_000142d8 + 0x142bc)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== OnPreShutdown  @ 000142e0 ===== */

/* easel::ApplicationBase::OnPreShutdown(bool&) */

void __thiscall easel::ApplicationBase::OnPreShutdown(ApplicationBase *this,bool *param_1)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_0001430c + 0x142ec);
  *param_1 = true;
  if (**(int **)(DAT_00014310 + 0x142fc) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== OnPreResetSetting  @ 00014314 ===== */

/* easel::ApplicationBase::OnPreResetSetting() */

void easel::ApplicationBase::OnPreResetSetting(void)

{
  if (**(int **)(DAT_00014340 + 0x1432c) != **(int **)(DAT_0001433c + 0x14320)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== OnResetSetting  @ 00014344 ===== */

/* easel::ApplicationBase::OnResetSetting() */

void easel::ApplicationBase::OnResetSetting(void)

{
  if (**(int **)(DAT_00014370 + 0x1435c) != **(int **)(DAT_0001436c + 0x14350)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== OnPostResetSetting  @ 00014374 ===== */

/* easel::ApplicationBase::OnPostResetSetting() */

void easel::ApplicationBase::OnPostResetSetting(void)

{
  if (**(int **)(DAT_000143a0 + 0x1438c) != **(int **)(DAT_0001439c + 0x14380)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== ReadyToSuspend  @ 000143a4 ===== */

/* easel::ApplicationBase::ReadyToSuspend() */

void __thiscall easel::ApplicationBase::ReadyToSuspend(ApplicationBase *this)

{
  ModuleRegistry *this_00;
  int iVar1;
  int iVar2;
  int iVar3;
  
  iVar3 = **(int **)(DAT_00014420 + 0x143b0);
  this_00 = (ModuleRegistry *)LifeCycleManager::GetModuleRegistry(*(LifeCycleManager **)(this + 4));
  ModuleRegistry::DoGetModule(this_00,*(uint *)(DAT_00014424 + 0x143c0));
  iVar1 = AppManagerModule::GetAppManager();
  std::mutex::lock();
  if ((0 < *(int *)(iVar1 + 0x168)) &&
     (iVar2 = *(int *)(iVar1 + 0x168) + -1, *(int *)(iVar1 + 0x168) = iVar2, iVar2 == 0)) {
    if (*(int *)(iVar1 + 0x130) != 0) {
      pst::appmanager::AppManager::Counter::StopTimeoutCheck();
    }
    if (*(int **)(iVar1 + 0x118) != (int *)0x0) {
      (**(code **)(**(int **)(iVar1 + 0x118) + 0x18))();
    }
  }
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00014428 + 0x14408) != iVar3) {
    __stack_chk_fail();
  }
                    /* WARNING: Could not recover jumptable at 0x000177a4. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (*(code *)(DAT_000177a8 + 0x177ac))(iVar1 + 0xd8);
  return;
}



/* ===== ReadyToEarlySuspend  @ 0001442c ===== */

/* easel::ApplicationBase::ReadyToEarlySuspend() */

void __thiscall easel::ApplicationBase::ReadyToEarlySuspend(ApplicationBase *this)

{
  ModuleRegistry *this_00;
  int iVar1;
  int iVar2;
  int iVar3;
  
  iVar3 = **(int **)(DAT_000144a8 + 83000);
  this_00 = (ModuleRegistry *)LifeCycleManager::GetModuleRegistry(*(LifeCycleManager **)(this + 4));
  ModuleRegistry::DoGetModule(this_00,*(uint *)(DAT_000144ac + 0x14448));
  iVar1 = AppManagerModule::GetAppManager();
  std::mutex::lock();
  if ((0 < *(int *)(iVar1 + 0x200)) &&
     (iVar2 = *(int *)(iVar1 + 0x200) + -1, *(int *)(iVar1 + 0x200) = iVar2, iVar2 == 0)) {
    if (*(int *)(iVar1 + 0x1c8) != 0) {
      pst::appmanager::AppManager::Counter::StopTimeoutCheck();
    }
    if (*(int **)(iVar1 + 0x1b0) != (int *)0x0) {
      (**(code **)(**(int **)(iVar1 + 0x1b0) + 0x18))();
    }
  }
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_000144b0 + 0x14490) != iVar3) {
    __stack_chk_fail();
  }
                    /* WARNING: Could not recover jumptable at 0x000177a4. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (*(code *)(DAT_000177a8 + 0x177ac))(iVar1 + 0x170);
  return;
}



/* ===== ReadyToShutdown  @ 000144b4 ===== */

/* easel::ApplicationBase::ReadyToShutdown() */

void __thiscall easel::ApplicationBase::ReadyToShutdown(ApplicationBase *this)

{
  ModuleRegistry *this_00;
  int iVar1;
  int iVar2;
  int iVar3;
  
  iVar3 = **(int **)(DAT_00014530 + 0x144c0);
  this_00 = (ModuleRegistry *)LifeCycleManager::GetModuleRegistry(*(LifeCycleManager **)(this + 4));
  ModuleRegistry::DoGetModule(this_00,*(uint *)(DAT_00014534 + 0x144d0));
  iVar1 = AppManagerModule::GetAppManager();
  std::mutex::lock();
  if ((0 < *(int *)(iVar1 + 0x298)) &&
     (iVar2 = *(int *)(iVar1 + 0x298) + -1, *(int *)(iVar1 + 0x298) = iVar2, iVar2 == 0)) {
    if (*(int *)(iVar1 + 0x260) != 0) {
      pst::appmanager::AppManager::Counter::StopTimeoutCheck();
    }
    if (*(int **)(iVar1 + 0x248) != (int *)0x0) {
      (**(code **)(**(int **)(iVar1 + 0x248) + 0x18))();
    }
  }
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00014538 + 0x14518) != iVar3) {
    __stack_chk_fail();
  }
                    /* WARNING: Could not recover jumptable at 0x000177a4. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (*(code *)(DAT_000177a8 + 0x177ac))(iVar1 + 0x208);
  return;
}



/* ===== ParseResumeFactorString  @ 0001453c ===== */

/* easel::ApplicationBase::ParseResumeFactorString(std::string const&, std::string&, std::string*)
    */

void easel::ApplicationBase::ParseResumeFactorString
               (string *param_1,string *param_2,string *param_3)

{
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00014570 + 0x14556) == **(int **)(DAT_0001456c + 0x14548)) {
    (*(code *)(DAT_000177b8 + 0x177bc))(param_1);
    return;
  }
  __stack_chk_fail();
}



/* ===== run  @ 00014574 ===== */

/* easel::ApplicationBase::run(int, char**, char const*, std::unique_ptr<easel::ModuleBaseInterface,
   std::default_delete<easel::ModuleBaseInterface> >) */

void __thiscall
easel::ApplicationBase::run
          (ApplicationBase *this,int param_1,char **param_2,char *param_3,undefined4 *param_5)

{
  int iVar1;
  int iVar2;
  int *piVar3;
  AppManagerModule *pAVar4;
  AppManager *pAVar5;
  ModuleRegistry *pMVar6;
  int iVar7;
  code *pcVar8;
  int iVar9;
  ChangeLifeCycleHandler *pCVar10;
  int iVar11;
  bool bVar12;
  ModuleRegistry *local_f4;
  int *local_f0;
  AppManagerModule *local_ec;
  int local_e8;
  ApplicationBase *pAStack_e4;
  int *local_d8;
  int local_d0;
  ApplicationBase *pAStack_cc;
  int *local_c0;
  int local_b8;
  ApplicationBase *pAStack_b4;
  int *local_a8;
  int local_a0;
  ApplicationBase *pAStack_9c;
  int *local_90;
  int local_88;
  ApplicationBase *pAStack_84;
  int *local_78;
  int local_70;
  ApplicationBase *pAStack_6c;
  int *local_60;
  int local_58;
  ApplicationBase *pAStack_54;
  int *local_48;
  int local_40;
  ApplicationBase *pAStack_3c;
  int *local_30;
  int local_28;
  
  local_28 = **(int **)(DAT_00014770 + 0x14588);
  pCVar10 = *(ChangeLifeCycleHandler **)(this + 4);
  pAVar4 = operator_new(0x20);
  AppManagerModule::AppManagerModule(pAVar4,param_1,param_2,param_3,pCVar10);
  iVar2 = DAT_00014784;
  iVar11 = DAT_00014780;
  iVar1 = DAT_0001477c;
  iVar7 = DAT_00014778;
  iVar9 = DAT_00014774 + 0x145bc;
  pAVar5 = (AppManager *)AppManagerModule::GetAppManager();
  local_58 = iVar2 + 0x145cc;
  local_70 = iVar11 + 0x145c6;
  local_88 = iVar7 + 0x145c0;
  local_a0 = iVar1 + 0x145c4;
  pAStack_9c = this;
  local_90 = &local_a0;
  pAStack_84 = this;
  local_78 = &local_88;
  pAStack_6c = this;
  local_60 = &local_70;
  pAStack_54 = this;
  local_48 = &local_58;
  local_40 = iVar9;
  pAStack_3c = this;
  local_30 = &local_40;
  pst::appmanager::AppManager::SetPowerStateHandler
            (pAVar5,&local_40,&local_58,&local_70,&local_88,&local_a0);
  if (local_90 == &local_a0) {
    pcVar8 = *(code **)(*local_90 + 0x10);
LAB_00014624:
    (*pcVar8)();
  }
  else if (local_90 != (int *)0x0) {
    pcVar8 = *(code **)(*local_90 + 0x14);
    goto LAB_00014624;
  }
  if (local_78 == &local_88) {
    pcVar8 = *(code **)(*local_78 + 0x10);
LAB_00014638:
    (*pcVar8)();
  }
  else if (local_78 != (int *)0x0) {
    pcVar8 = *(code **)(*local_78 + 0x14);
    goto LAB_00014638;
  }
  if (local_60 == &local_70) {
    pcVar8 = *(code **)(*local_60 + 0x10);
LAB_0001464c:
    (*pcVar8)();
  }
  else if (local_60 != (int *)0x0) {
    pcVar8 = *(code **)(*local_60 + 0x14);
    goto LAB_0001464c;
  }
  if (local_48 == &local_58) {
    pcVar8 = *(code **)(*local_48 + 0x10);
LAB_00014660:
    (*pcVar8)();
  }
  else if (local_48 != (int *)0x0) {
    pcVar8 = *(code **)(*local_48 + 0x14);
    goto LAB_00014660;
  }
  if (local_30 == &local_40) {
    pcVar8 = *(code **)(*local_30 + 0x10);
LAB_00014674:
    (*pcVar8)();
  }
  else if (local_30 != (int *)0x0) {
    pcVar8 = *(code **)(*local_30 + 0x14);
    goto LAB_00014674;
  }
  iVar1 = DAT_00014790;
  iVar7 = DAT_0001478c;
  iVar11 = DAT_00014788 + 0x14688;
  pAVar5 = (AppManager *)AppManagerModule::GetAppManager();
  local_d0 = iVar1 + 0x14690;
  local_e8 = iVar7 + 0x1468a;
  pAStack_e4 = this;
  local_d8 = &local_e8;
  pAStack_cc = this;
  local_c0 = &local_d0;
  local_b8 = iVar11;
  pAStack_b4 = this;
  local_a8 = &local_b8;
  pst::appmanager::AppManager::SetResetSettingHandler(pAVar5,&local_b8,&local_d0,&local_e8);
  if (local_d8 == &local_e8) {
    pcVar8 = *(code **)(*local_d8 + 0x10);
LAB_000146c6:
    (*pcVar8)();
  }
  else if (local_d8 != (int *)0x0) {
    pcVar8 = *(code **)(*local_d8 + 0x14);
    goto LAB_000146c6;
  }
  if (local_c0 == &local_d0) {
    pcVar8 = *(code **)(*local_c0 + 0x10);
LAB_000146da:
    (*pcVar8)();
  }
  else if (local_c0 != (int *)0x0) {
    pcVar8 = *(code **)(*local_c0 + 0x14);
    goto LAB_000146da;
  }
  if (local_a8 == &local_b8) {
    pcVar8 = *(code **)(*local_a8 + 0x10);
  }
  else {
    if (local_a8 == (int *)0x0) goto LAB_000146f0;
    pcVar8 = *(code **)(*local_a8 + 0x14);
  }
  (*pcVar8)();
LAB_000146f0:
  pMVar6 = operator_new(0x10);
  ModuleRegistry::ModuleRegistry(pMVar6);
  local_ec = pAVar4;
  ModuleRegistry::Register(pMVar6,&local_ec);
  pAVar4 = local_ec;
  local_ec = (AppManagerModule *)0x0;
  if (pAVar4 != (AppManagerModule *)0x0) {
    (**(code **)(*(int *)pAVar4 + 4))();
  }
  local_f0 = (int *)*param_5;
  *param_5 = 0;
  ModuleRegistry::Register(pMVar6,&local_f0);
  piVar3 = local_f0;
  local_f0 = (int *)0x0;
  if (piVar3 != (int *)0x0) {
    (**(code **)(*piVar3 + 4))();
  }
  local_f4 = pMVar6;
  LifeCycleManager::Main(*(LifeCycleManager **)(this + 4),&local_f4);
  pMVar6 = local_f4;
  bVar12 = local_f4 != (ModuleRegistry *)0x0;
  local_f4 = (ModuleRegistry *)0x0;
  if (bVar12) {
    (**(code **)(*(int *)pMVar6 + 4))();
  }
  iVar7 = **(int **)(DAT_00014794 + 0x1475c) - local_28;
  if (iVar7 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar7);
  }
  return;
}



/* ===== SetPowerStateHandler  @ 00014798 ===== */

/* pst::appmanager::AppManager::SetPowerStateHandler(std::function<void (bool&)>, std::function<void
   (bool&)>, std::function<void (bool&)>, std::function<void (std::string const&)>,
   std::function<void (std::string const&)>) */

void __thiscall
pst::appmanager::AppManager::SetPowerStateHandler
          (AppManager *this,int *param_2,int *param_3,int *param_4,int *param_5,int *param_6)

{
  int *piVar1;
  code *pcVar2;
  function<void(bool&)> afStack_38 [16];
  int *local_28;
  int local_20;
  
  local_28 = (int *)afStack_38;
  local_20 = **(int **)(DAT_00014910 + 0x147ae);
  piVar1 = (int *)param_2[4];
  if (piVar1 == (int *)0x0) {
    local_28 = (int *)0x0;
  }
  else if (piVar1 == param_2) {
    (**(code **)(*piVar1 + 0xc))();
  }
  else {
    local_28 = (int *)(**(code **)(*piVar1 + 8))();
  }
  std::function<void(bool&)>::swap(afStack_38,this + 0x18);
  if ((function<void(bool&)> *)local_28 == afStack_38) {
    pcVar2 = *(code **)(*local_28 + 0x10);
LAB_000147f0:
    (*pcVar2)();
  }
  else if (local_28 != (int *)0x0) {
    pcVar2 = *(code **)(*local_28 + 0x14);
    goto LAB_000147f0;
  }
  piVar1 = (int *)param_3[4];
  if (piVar1 == (int *)0x0) {
    local_28 = (int *)0x0;
  }
  else if (piVar1 == param_3) {
    local_28 = (int *)afStack_38;
    (**(code **)(*piVar1 + 0xc))(piVar1,afStack_38);
  }
  else {
    local_28 = (int *)(**(code **)(*piVar1 + 8))();
  }
  std::function<void(bool&)>::swap(afStack_38,this + 0x30);
  if ((function<void(bool&)> *)local_28 == afStack_38) {
    pcVar2 = *(code **)(*local_28 + 0x10);
LAB_00014830:
    (*pcVar2)();
  }
  else if (local_28 != (int *)0x0) {
    pcVar2 = *(code **)(*local_28 + 0x14);
    goto LAB_00014830;
  }
  piVar1 = (int *)param_4[4];
  if (piVar1 == (int *)0x0) {
    local_28 = (int *)0x0;
  }
  else if (piVar1 == param_4) {
    local_28 = (int *)afStack_38;
    (**(code **)(*piVar1 + 0xc))(piVar1,afStack_38);
  }
  else {
    local_28 = (int *)(**(code **)(*piVar1 + 8))();
  }
  std::function<void(bool&)>::swap(afStack_38,this + 0x48);
  if ((function<void(bool&)> *)local_28 == afStack_38) {
    pcVar2 = *(code **)(*local_28 + 0x10);
LAB_00014874:
    (*pcVar2)();
  }
  else if (local_28 != (int *)0x0) {
    pcVar2 = *(code **)(*local_28 + 0x14);
    goto LAB_00014874;
  }
  piVar1 = (int *)param_5[4];
  if (piVar1 == (int *)0x0) {
    local_28 = (int *)0x0;
  }
  else if (piVar1 == param_5) {
    local_28 = (int *)afStack_38;
    (**(code **)(*piVar1 + 0xc))(piVar1,afStack_38);
  }
  else {
    local_28 = (int *)(**(code **)(*piVar1 + 8))();
  }
  std::function<void(std::string_const&)>::swap
            ((function<void(std::string_const&)> *)afStack_38,this + 0x60);
  if ((function<void(bool&)> *)local_28 == afStack_38) {
    pcVar2 = *(code **)(*local_28 + 0x10);
LAB_000148b6:
    (*pcVar2)();
  }
  else if (local_28 != (int *)0x0) {
    pcVar2 = *(code **)(*local_28 + 0x14);
    goto LAB_000148b6;
  }
  piVar1 = (int *)param_6[4];
  if (piVar1 == (int *)0x0) {
    local_28 = (int *)0x0;
  }
  else if (piVar1 == param_6) {
    local_28 = (int *)afStack_38;
    (**(code **)(*piVar1 + 0xc))(piVar1,afStack_38);
  }
  else {
    local_28 = (int *)(**(code **)(*piVar1 + 8))();
  }
  std::function<void(std::string_const&)>::swap
            ((function<void(std::string_const&)> *)afStack_38,this + 0x78);
  if ((function<void(bool&)> *)local_28 == afStack_38) {
    pcVar2 = *(code **)(*local_28 + 0x10);
  }
  else {
    if (local_28 == (int *)0x0) goto LAB_000148f8;
    pcVar2 = *(code **)(*local_28 + 0x14);
  }
  (*pcVar2)();
LAB_000148f8:
  if (**(int **)(DAT_00014914 + 0x14900) == local_20) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail();
}



/* ===== SetResetSettingHandler  @ 00014918 ===== */

/* pst::appmanager::AppManager::SetResetSettingHandler(std::function<void ()>, std::function<void
   ()>, std::function<void ()>) */

void __thiscall
pst::appmanager::AppManager::SetResetSettingHandler
          (AppManager *this,int *param_2,int *param_3,int *param_4)

{
  int *piVar1;
  code *pcVar2;
  function<void()> afStack_38 [16];
  int *local_28;
  int local_20;
  
  local_28 = (int *)afStack_38;
  local_20 = **(int **)(DAT_00014a0c + 0x1492e);
  piVar1 = (int *)param_2[4];
  if (piVar1 == (int *)0x0) {
    local_28 = (int *)0x0;
  }
  else if (piVar1 == param_2) {
    (**(code **)(*piVar1 + 0xc))();
  }
  else {
    local_28 = (int *)(**(code **)(*piVar1 + 8))();
  }
  std::function<void()>::swap(afStack_38,this + 0x90);
  if ((function<void()> *)local_28 == afStack_38) {
    pcVar2 = *(code **)(*local_28 + 0x10);
LAB_00014970:
    (*pcVar2)();
  }
  else if (local_28 != (int *)0x0) {
    pcVar2 = *(code **)(*local_28 + 0x14);
    goto LAB_00014970;
  }
  piVar1 = (int *)param_3[4];
  if (piVar1 == (int *)0x0) {
    local_28 = (int *)0x0;
  }
  else if (piVar1 == param_3) {
    local_28 = (int *)afStack_38;
    (**(code **)(*piVar1 + 0xc))(piVar1,afStack_38);
  }
  else {
    local_28 = (int *)(**(code **)(*piVar1 + 8))();
  }
  std::function<void()>::swap(afStack_38,this + 0xa8);
  if ((function<void()> *)local_28 == afStack_38) {
    pcVar2 = *(code **)(*local_28 + 0x10);
LAB_000149b0:
    (*pcVar2)();
  }
  else if (local_28 != (int *)0x0) {
    pcVar2 = *(code **)(*local_28 + 0x14);
    goto LAB_000149b0;
  }
  piVar1 = (int *)param_4[4];
  if (piVar1 == (int *)0x0) {
    local_28 = (int *)0x0;
  }
  else if (piVar1 == param_4) {
    local_28 = (int *)afStack_38;
    (**(code **)(*piVar1 + 0xc))(piVar1,afStack_38);
  }
  else {
    local_28 = (int *)(**(code **)(*piVar1 + 8))();
  }
  std::function<void()>::swap(afStack_38,this + 0xc0);
  if ((function<void()> *)local_28 == afStack_38) {
    pcVar2 = *(code **)(*local_28 + 0x10);
  }
  else {
    if (local_28 == (int *)0x0) goto LAB_000149f4;
    pcVar2 = *(code **)(*local_28 + 0x14);
  }
  (*pcVar2)();
LAB_000149f4:
  if (**(int **)(DAT_00014a10 + 0x149fc) == local_20) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail();
}



/* ===== StopBootAnimation  @ 00014a14 ===== */

/* easel::ApplicationBase::StopBootAnimation() */

void easel::ApplicationBase::StopBootAnimation(void)

{
  int iVar1;
  int iVar2;
  
  iVar2 = **(int **)(DAT_00014a68 + 0x14a20);
  iVar1 = FUN_00014a80(DAT_00014a6c + 0x14a2a);
  if (iVar1 == -1) {
    pst::log::Print(3,DAT_00014a70 + 0x14a44,0xe5,0,DAT_00014a74 + 0x14a46,DAT_00014a78 + 0x14a42);
  }
  if (**(int **)(DAT_00014a7c + 0x14a5a) == iVar2) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail(iVar1 == -1);
}



/* ===== FUN_00014a80  @ 00014a80 ===== */

void FUN_00014a80(int param_1)

{
  int iVar1;
  __pid_t __pid;
  int iVar2;
  int iVar3;
  
  iVar1 = **(int **)(DAT_00014b54 + 0x14a92);
  if (param_1 == 0) {
    pst::log::Print(3,DAT_00014b78 + 0x14af6,0xc4,0,DAT_00014b7c + 0x14af8,DAT_00014b80 + 0x14af4);
  }
  else {
    __pid = fork();
    if (-1 < __pid) {
      if (__pid == 0) {
        pst::log::Print(6,DAT_00014b68 + 0x14b30,0xce,0,DAT_00014b6c + 0x14b32,
                        DAT_00014b70 + 0x14b34,param_1);
        execlp((char *)(DAT_00014b74 + 0x14b48),(char *)(DAT_00014b74 + 0x14b48),param_1,0);
                    /* WARNING: Subroutine does not return */
        _exit(-1);
      }
      iVar3 = DAT_00014b58 + 0x14ab4;
      iVar2 = DAT_00014b5c + 0x14aba;
      pst::log::Print(6,iVar3,0xd7,0,iVar2,DAT_00014b60 + 0x14ab6,__pid);
      waitpid(__pid,(int *)0x0,0);
      pst::log::Print(6,iVar3,0xd9,0,iVar2,DAT_00014b64 + 0x14adc,__pid);
    }
  }
  iVar1 = **(int **)(DAT_00014b84 + 0x14b12) - iVar1;
  if (iVar1 == 0) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail(iVar1);
}



/* ===== StartResumeAnimation  @ 00014b88 ===== */

/* easel::ApplicationBase::StartResumeAnimation() */

void easel::ApplicationBase::StartResumeAnimation(void)

{
  __pid_t _Var1;
  undefined4 uVar2;
  undefined8 local_18;
  undefined4 local_10;
  int local_c;
  
  local_18 = *(undefined8 *)(DAT_00014c04 + 0x14b98);
  local_c = **(int **)(DAT_00014c00 + 0x14b96);
  local_10 = *(undefined4 *)(DAT_00014c04 + 0x14ba0);
  _Var1 = fork();
  if (_Var1 < 0) {
    pst::log::Print(3,DAT_00014c0c + 0x14bc4,0x10a,0,DAT_00014c10 + 0x14bc6,DAT_00014c14 + 0x14bc2);
    uVar2 = 1;
  }
  else {
    if (_Var1 == 0) {
      execve((char *)(DAT_00014c08 + 0x14bf4),(char **)&local_18,(char **)0x0);
                    /* WARNING: Subroutine does not return */
      _exit(-1);
    }
    uVar2 = 0;
  }
  if (**(int **)(DAT_00014c18 + 0x14bdc) != local_c) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(uVar2);
  }
  return;
}



/* ===== StopResumeAnimation  @ 00014c1c ===== */

/* easel::ApplicationBase::StopResumeAnimation() */

void easel::ApplicationBase::StopResumeAnimation(void)

{
  int iVar1;
  int iVar2;
  
  iVar2 = **(int **)(DAT_00014c74 + 0x14c28);
  iVar1 = FUN_00014a80(DAT_00014c78 + 0x14c32);
  if (iVar1 == -1) {
    pst::log::Print(3,DAT_00014c7c + 0x14c4c,0x115,0,DAT_00014c80 + 0x14c4e,DAT_00014c84 + 0x14c4a);
  }
  if (**(int **)(DAT_00014c88 + 0x14c64) == iVar2) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail(iVar1 == -1);
}



/* ===== GetPostInitializeTimeout  @ 00014c8c ===== */

/* easel::ApplicationBase::GetPostInitializeTimeout() */

void easel::ApplicationBase::GetPostInitializeTimeout(void)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00014cbc + 0x14ca4) - **(int **)(DAT_00014cb8 + 0x14c98);
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1,**(int **)(DAT_00014cb8 + 0x14c98));
  }
  return;
}



/* ===== GetChangeLifeCycleTimeout  @ 00014cc0 ===== */

/* easel::ApplicationBase::GetChangeLifeCycleTimeout() */

void easel::ApplicationBase::GetChangeLifeCycleTimeout(void)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00014cf0 + 0x14cd8) - **(int **)(DAT_00014cec + 0x14ccc);
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1,**(int **)(DAT_00014cec + 0x14ccc));
  }
  return;
}



/* ===== swap  @ 00014cf4 ===== */

/* std::function<void (bool&)>::swap(std::function<void (bool&)>&) */

void __thiscall std::function<void(bool&)>::swap(function<void(bool&)> *this,function *param_1)

{
  function<void(bool&)> *pfVar1;
  function *pfVar2;
  int *piVar3;
  int local_30 [5];
  int local_1c;
  
  local_1c = **(int **)(DAT_00014db0 + 0x14d06);
  pfVar1 = *(function<void(bool&)> **)(this + 0x10);
  pfVar2 = param_1;
  if (pfVar1 == this) {
    pfVar2 = *(function **)(param_1 + 0x10);
  }
  if (pfVar1 == this && pfVar2 == param_1) {
    (**(code **)(*(int *)pfVar1 + 0xc))(pfVar1,local_30);
    (**(code **)(**(int **)(this + 0x10) + 0x10))();
    *(undefined4 *)(this + 0x10) = 0;
    (**(code **)(**(int **)(param_1 + 0x10) + 0xc))(*(int **)(param_1 + 0x10),this);
    (**(code **)(**(int **)(param_1 + 0x10) + 0x10))();
    *(undefined4 *)(param_1 + 0x10) = 0;
    *(function<void(bool&)> **)(this + 0x10) = this;
    (**(code **)(local_30[0] + 0xc))(local_30,param_1);
    (**(code **)(local_30[0] + 0x10))(local_30);
    *(function **)(param_1 + 0x10) = param_1;
  }
  else if (pfVar1 == this) {
    (**(code **)(*(int *)pfVar1 + 0xc))(pfVar1,param_1);
    (**(code **)(**(int **)(this + 0x10) + 0x10))();
    *(undefined4 *)(this + 0x10) = *(undefined4 *)(param_1 + 0x10);
    *(function **)(param_1 + 0x10) = param_1;
  }
  else {
    piVar3 = *(int **)(param_1 + 0x10);
    if (piVar3 == (int *)param_1) {
      (**(code **)(*piVar3 + 0xc))(piVar3,this);
      (**(code **)(**(int **)(param_1 + 0x10) + 0x10))();
      *(undefined4 *)(param_1 + 0x10) = *(undefined4 *)(this + 0x10);
      *(function<void(bool&)> **)(this + 0x10) = this;
    }
    else {
      *(int **)(this + 0x10) = piVar3;
      *(function<void(bool&)> **)(param_1 + 0x10) = pfVar1;
    }
  }
  if (**(int **)(DAT_00014db4 + 0x14da0) != local_1c) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== swap  @ 00014db8 ===== */

/* std::function<void (std::string const&)>::swap(std::function<void (std::string const&)>&) */

void __thiscall
std::function<void(std::string_const&)>::swap
          (function<void(std::string_const&)> *this,function *param_1)

{
  function<void(std::string_const&)> *pfVar1;
  function *pfVar2;
  int *piVar3;
  int local_30 [5];
  int local_1c;
  
  local_1c = **(int **)(DAT_00014e74 + 0x14dca);
  pfVar1 = *(function<void(std::string_const&)> **)(this + 0x10);
  pfVar2 = param_1;
  if (pfVar1 == this) {
    pfVar2 = *(function **)(param_1 + 0x10);
  }
  if (pfVar1 == this && pfVar2 == param_1) {
    (**(code **)(*(int *)pfVar1 + 0xc))(pfVar1,local_30);
    (**(code **)(**(int **)(this + 0x10) + 0x10))();
    *(undefined4 *)(this + 0x10) = 0;
    (**(code **)(**(int **)(param_1 + 0x10) + 0xc))(*(int **)(param_1 + 0x10),this);
    (**(code **)(**(int **)(param_1 + 0x10) + 0x10))();
    *(undefined4 *)(param_1 + 0x10) = 0;
    *(function<void(std::string_const&)> **)(this + 0x10) = this;
    (**(code **)(local_30[0] + 0xc))(local_30,param_1);
    (**(code **)(local_30[0] + 0x10))(local_30);
    *(function **)(param_1 + 0x10) = param_1;
  }
  else if (pfVar1 == this) {
    (**(code **)(*(int *)pfVar1 + 0xc))(pfVar1,param_1);
    (**(code **)(**(int **)(this + 0x10) + 0x10))();
    *(undefined4 *)(this + 0x10) = *(undefined4 *)(param_1 + 0x10);
    *(function **)(param_1 + 0x10) = param_1;
  }
  else {
    piVar3 = *(int **)(param_1 + 0x10);
    if (piVar3 == (int *)param_1) {
      (**(code **)(*piVar3 + 0xc))(piVar3,this);
      (**(code **)(**(int **)(param_1 + 0x10) + 0x10))();
      *(undefined4 *)(param_1 + 0x10) = *(undefined4 *)(this + 0x10);
      *(function<void(std::string_const&)> **)(this + 0x10) = this;
    }
    else {
      *(int **)(this + 0x10) = piVar3;
      *(function<void(std::string_const&)> **)(param_1 + 0x10) = pfVar1;
    }
  }
  if (**(int **)(DAT_00014e78 + 0x14e64) != local_1c) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== swap  @ 00014e7c ===== */

/* std::function<void ()>::swap(std::function<void ()>&) */

void __thiscall std::function<void()>::swap(function<void()> *this,function *param_1)

{
  function<void()> *pfVar1;
  function *pfVar2;
  int *piVar3;
  int local_30 [5];
  int local_1c;
  
  local_1c = **(int **)(DAT_00014f38 + 0x14e8e);
  pfVar1 = *(function<void()> **)(this + 0x10);
  pfVar2 = param_1;
  if (pfVar1 == this) {
    pfVar2 = *(function **)(param_1 + 0x10);
  }
  if (pfVar1 == this && pfVar2 == param_1) {
    (**(code **)(*(int *)pfVar1 + 0xc))(pfVar1,local_30);
    (**(code **)(**(int **)(this + 0x10) + 0x10))();
    *(undefined4 *)(this + 0x10) = 0;
    (**(code **)(**(int **)(param_1 + 0x10) + 0xc))(*(int **)(param_1 + 0x10),this);
    (**(code **)(**(int **)(param_1 + 0x10) + 0x10))();
    *(undefined4 *)(param_1 + 0x10) = 0;
    *(function<void()> **)(this + 0x10) = this;
    (**(code **)(local_30[0] + 0xc))(local_30,param_1);
    (**(code **)(local_30[0] + 0x10))(local_30);
    *(function **)(param_1 + 0x10) = param_1;
  }
  else if (pfVar1 == this) {
    (**(code **)(*(int *)pfVar1 + 0xc))(pfVar1,param_1);
    (**(code **)(**(int **)(this + 0x10) + 0x10))();
    *(undefined4 *)(this + 0x10) = *(undefined4 *)(param_1 + 0x10);
    *(function **)(param_1 + 0x10) = param_1;
  }
  else {
    piVar3 = *(int **)(param_1 + 0x10);
    if (piVar3 == (int *)param_1) {
      (**(code **)(*piVar3 + 0xc))(piVar3,this);
      (**(code **)(**(int **)(param_1 + 0x10) + 0x10))();
      *(undefined4 *)(param_1 + 0x10) = *(undefined4 *)(this + 0x10);
      *(function<void()> **)(this + 0x10) = this;
    }
    else {
      *(int **)(this + 0x10) = piVar3;
      *(function<void()> **)(param_1 + 0x10) = pfVar1;
    }
  }
  if (**(int **)(DAT_00014f3c + 0x14f28) != local_1c) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_00014f40  @ 00014f40 ===== */

void FUN_00014f40(void)

{
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00014f70 + 0x14f58) == **(int **)(DAT_00014f6c + 0x14f4c)) {
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00017788 + 0x1778c))();
    return;
  }
  __stack_chk_fail();
}



/* ===== FUN_00014f74  @ 00014f74 ===== */

void FUN_00014f74(int param_1)

{
  int iVar1;
  int *piVar2;
  
  iVar1 = **(int **)(DAT_00014fb0 + 0x14f82);
  piVar2 = operator_new(8);
  *piVar2 = DAT_00014fb4 + 0x14f9a;
  piVar2[1] = *(int *)(param_1 + 4);
  if (**(int **)(DAT_00014fb8 + 0x14fa0) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_00014fbc  @ 00014fbc ===== */

void FUN_00014fbc(int param_1,int *param_2)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00014ff0 + 0x14fca);
  *param_2 = DAT_00014ff4 + 0x14fd4;
  param_2[1] = *(int *)(param_1 + 4);
  if (**(int **)(DAT_00014ff8 + 0x14fe2) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_00014ffc  @ 00014ffc ===== */

void FUN_00014ffc(void)

{
  if (**(int **)(DAT_00015028 + 0x15014) != **(int **)(DAT_00015024 + 0x15008)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_0001502c  @ 0001502c ===== */

void FUN_0001502c(void)

{
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_0001505c + 0x15044) == **(int **)(DAT_00015058 + 0x15038)) {
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00017788 + 0x1778c))();
    return;
  }
  __stack_chk_fail();
}



/* ===== FUN_00015060  @ 00015060 ===== */

void FUN_00015060(int param_1,undefined4 param_2)

{
  int iVar1;
  int *piVar2;
  
  iVar1 = **(int **)(DAT_000150b0 + 0x15070);
  piVar2 = *(int **)(param_1 + 4);
  pst::log::Print(5,DAT_000150b8 + 0x15084,0x8d,0,DAT_000150b4 + 0x15072,DAT_000150bc + 0x15082);
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_000150c0 + 0x1509a) != iVar1) {
    __stack_chk_fail();
  }
                    /* WARNING: Could not recover jumptable at 0x000150ae. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (**(code **)(*piVar2 + 0x2c))(piVar2,param_2);
  return;
}



/* ===== FUN_000150c4  @ 000150c4 ===== */

void FUN_000150c4(void)

{
  if (**(int **)(DAT_000150f0 + 0x150dc) != **(int **)(DAT_000150ec + 0x150d0)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_00015170  @ 00015170 ===== */

void FUN_00015170(int param_1,int *param_2)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_000151a4 + 0x1517e);
  *param_2 = DAT_000151a8 + 0x15188;
  param_2[1] = *(int *)(param_1 + 4);
  if (**(int **)(DAT_000151ac + 0x15196) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_000151b0  @ 000151b0 ===== */

void FUN_000151b0(void)

{
  if (**(int **)(DAT_000151dc + 0x151c8) != **(int **)(DAT_000151d8 + 0x151bc)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_000151e0  @ 000151e0 ===== */

void FUN_000151e0(void)

{
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00015210 + 0x151f8) == **(int **)(DAT_0001520c + 0x151ec)) {
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00017788 + 0x1778c))();
    return;
  }
  __stack_chk_fail();
}



/* ===== FUN_00015214  @ 00015214 ===== */

void FUN_00015214(int param_1,undefined4 param_2)

{
  int iVar1;
  int *piVar2;
  
  iVar1 = **(int **)(DAT_00015264 + 0x15224);
  piVar2 = *(int **)(param_1 + 4);
  pst::log::Print(5,DAT_0001526c + 0x15238,0x91,0,DAT_00015268 + 0x15226,DAT_00015270 + 0x15236);
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00015274 + 0x1524e) != iVar1) {
    __stack_chk_fail();
  }
                    /* WARNING: Could not recover jumptable at 0x00015262. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (**(code **)(*piVar2 + 0x24))(piVar2,param_2);
  return;
}



/* ===== FUN_000152f4  @ 000152f4 ===== */

void FUN_000152f4(int param_1,int *param_2)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00015328 + 0x15302);
  *param_2 = DAT_0001532c + 0x1530c;
  param_2[1] = *(int *)(param_1 + 4);
  if (**(int **)(DAT_00015330 + 0x1531a) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_00015334  @ 00015334 ===== */

void FUN_00015334(void)

{
  if (**(int **)(DAT_00015360 + 0x1534c) != **(int **)(DAT_0001535c + 0x15340)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_00015364  @ 00015364 ===== */

void FUN_00015364(void)

{
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00015394 + 0x1537c) == **(int **)(DAT_00015390 + 0x15370)) {
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00017788 + 0x1778c))();
    return;
  }
  __stack_chk_fail();
}



/* ===== FUN_00015398  @ 00015398 ===== */

void FUN_00015398(int param_1,undefined4 param_2)

{
  int iVar1;
  int *piVar2;
  
  iVar1 = **(int **)(DAT_000153e8 + 0x153a8);
  piVar2 = *(int **)(param_1 + 4);
  pst::log::Print(5,DAT_000153f0 + 0x153bc,0x95,0,DAT_000153ec + 0x153aa,DAT_000153f4 + 0x153ba);
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_000153f8 + 0x153d2) != iVar1) {
    __stack_chk_fail();
  }
                    /* WARNING: Could not recover jumptable at 0x000153e6. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (**(code **)(*piVar2 + 0x34))(piVar2,param_2);
  return;
}



/* ===== FUN_000153fc  @ 000153fc ===== */

void FUN_000153fc(void)

{
  if (**(int **)(DAT_00015428 + 0x15414) != **(int **)(DAT_00015424 + 0x15408)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_000154a8  @ 000154a8 ===== */

void FUN_000154a8(int param_1,int *param_2)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_000154dc + 0x154b6);
  *param_2 = DAT_000154e0 + 0x154c0;
  param_2[1] = *(int *)(param_1 + 4);
  if (**(int **)(DAT_000154e4 + 0x154ce) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_000154e8  @ 000154e8 ===== */

void FUN_000154e8(void)

{
  if (**(int **)(DAT_00015514 + 0x15500) != **(int **)(DAT_00015510 + 0x154f4)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_00015518  @ 00015518 ===== */

void FUN_00015518(void)

{
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00015548 + 0x15530) == **(int **)(DAT_00015544 + 0x15524)) {
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00017788 + 0x1778c))();
    return;
  }
  __stack_chk_fail();
}



/* ===== FUN_0001554c  @ 0001554c ===== */

void FUN_0001554c(int param_1,undefined4 param_2)

{
  int iVar1;
  int *piVar2;
  ulonglong uVar3;
  
  iVar1 = **(int **)(DAT_000155ac + 0x1555a);
  piVar2 = *(int **)(param_1 + 4);
  uVar3 = pst::core::Framework::GetReference();
  pst::core::Framework::UpdateCurrentContextHangTimeout(uVar3);
  pst::log::Print(5,DAT_000155b0 + 0x1557c,0x9b,0,DAT_000155b4 + 0x1557e,DAT_000155b8 + 0x1557a);
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_000155bc + 0x15594) != iVar1) {
    __stack_chk_fail();
  }
                    /* WARNING: Could not recover jumptable at 0x000155a8. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (**(code **)(*piVar2 + 0x28))(piVar2,param_2);
  return;
}



/* ===== FUN_0001563c  @ 0001563c ===== */

void FUN_0001563c(int param_1,int *param_2)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00015670 + 0x1564a);
  *param_2 = DAT_00015674 + 0x15654;
  param_2[1] = *(int *)(param_1 + 4);
  if (**(int **)(DAT_00015678 + 0x15662) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_0001567c  @ 0001567c ===== */

void FUN_0001567c(void)

{
  if (**(int **)(DAT_000156a8 + 0x15694) != **(int **)(DAT_000156a4 + 0x15688)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_000156ac  @ 000156ac ===== */

void FUN_000156ac(void)

{
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_000156dc + 0x156c4) == **(int **)(DAT_000156d8 + 0x156b8)) {
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00017788 + 0x1778c))();
    return;
  }
  __stack_chk_fail();
}



/* ===== FUN_000156e0  @ 000156e0 ===== */

void FUN_000156e0(int param_1,undefined4 param_2)

{
  int iVar1;
  int *piVar2;
  ulonglong uVar3;
  
  iVar1 = **(int **)(DAT_00015740 + 0x156ee);
  piVar2 = *(int **)(param_1 + 4);
  uVar3 = pst::core::Framework::GetReference();
  pst::core::Framework::UpdateCurrentContextHangTimeout(uVar3);
  pst::log::Print(5,DAT_00015744 + 0x15710,0xa1,0,DAT_00015748 + 0x15712,DAT_0001574c + 0x1570e);
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00015750 + 0x15728) != iVar1) {
    __stack_chk_fail();
  }
                    /* WARNING: Could not recover jumptable at 0x0001573c. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (**(code **)(*piVar2 + 0x30))(piVar2,param_2);
  return;
}



/* ===== FUN_00015754  @ 00015754 ===== */

void FUN_00015754(void)

{
  if (**(int **)(DAT_00015780 + 0x1576c) != **(int **)(DAT_0001577c + 0x15760)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_00015800  @ 00015800 ===== */

void FUN_00015800(int param_1,int *param_2)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00015834 + 0x1580e);
  *param_2 = DAT_00015838 + 0x15818;
  param_2[1] = *(int *)(param_1 + 4);
  if (**(int **)(DAT_0001583c + 0x15826) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_00015840  @ 00015840 ===== */

void FUN_00015840(void)

{
  if (**(int **)(DAT_0001586c + 0x15858) != **(int **)(DAT_00015868 + 0x1584c)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_00015870  @ 00015870 ===== */

void FUN_00015870(void)

{
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_000158a0 + 0x15888) == **(int **)(DAT_0001589c + 0x1587c)) {
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00017788 + 0x1778c))();
    return;
  }
  __stack_chk_fail();
}



/* ===== FUN_000158a4  @ 000158a4 ===== */

void FUN_000158a4(int param_1)

{
  int iVar1;
  int *piVar2;
  
  iVar1 = **(int **)(DAT_000158f0 + 0x158b2);
  piVar2 = *(int **)(param_1 + 4);
  pst::log::Print(5,DAT_000158f8 + 0x158c6,0xa6,0,DAT_000158f4 + 0x158b4,DAT_000158fc + 0x158c4);
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00015900 + 0x158dc) != iVar1) {
    __stack_chk_fail();
  }
                    /* WARNING: Could not recover jumptable at 0x000158ee. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (**(code **)(*piVar2 + 0x38))(piVar2);
  return;
}



/* ===== FUN_00015980  @ 00015980 ===== */

void FUN_00015980(int param_1,int *param_2)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_000159b4 + 0x1598e);
  *param_2 = DAT_000159b8 + 0x15998;
  param_2[1] = *(int *)(param_1 + 4);
  if (**(int **)(DAT_000159bc + 0x159a6) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_000159c0  @ 000159c0 ===== */

void FUN_000159c0(void)

{
  if (**(int **)(DAT_000159ec + 0x159d8) != **(int **)(DAT_000159e8 + 0x159cc)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_000159f0  @ 000159f0 ===== */

void FUN_000159f0(void)

{
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00015a20 + 0x15a08) == **(int **)(DAT_00015a1c + 0x159fc)) {
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00017788 + 0x1778c))();
    return;
  }
  __stack_chk_fail();
}



/* ===== FUN_00015a24  @ 00015a24 ===== */

void FUN_00015a24(int param_1)

{
  int iVar1;
  int *piVar2;
  
  iVar1 = **(int **)(DAT_00015a70 + 0x15a32);
  piVar2 = *(int **)(param_1 + 4);
  pst::log::Print(5,DAT_00015a78 + 0x15a46,0xaa,0,DAT_00015a74 + 0x15a34,DAT_00015a7c + 0x15a44);
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00015a80 + 0x15a5c) != iVar1) {
    __stack_chk_fail();
  }
                    /* WARNING: Could not recover jumptable at 0x00015a6e. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (**(code **)(*piVar2 + 0x3c))(piVar2);
  return;
}



/* ===== FUN_00015b00  @ 00015b00 ===== */

void FUN_00015b00(int param_1,int *param_2)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00015b34 + 0x15b0e);
  *param_2 = DAT_00015b38 + 0x15b18;
  param_2[1] = *(int *)(param_1 + 4);
  if (**(int **)(DAT_00015b3c + 0x15b26) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_00015b40  @ 00015b40 ===== */

void FUN_00015b40(void)

{
  if (**(int **)(DAT_00015b6c + 0x15b58) != **(int **)(DAT_00015b68 + 0x15b4c)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_00015b70  @ 00015b70 ===== */

void FUN_00015b70(void)

{
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00015ba0 + 0x15b88) == **(int **)(DAT_00015b9c + 0x15b7c)) {
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00017788 + 0x1778c))();
    return;
  }
  __stack_chk_fail();
}



/* ===== FUN_00015ba4  @ 00015ba4 ===== */

void FUN_00015ba4(int param_1)

{
  int iVar1;
  int *piVar2;
  
  iVar1 = **(int **)(DAT_00015bf0 + 0x15bb2);
  piVar2 = *(int **)(param_1 + 4);
  pst::log::Print(5,DAT_00015bf8 + 0x15bc6,0xae,0,DAT_00015bf4 + 0x15bb4,DAT_00015bfc + 0x15bc4);
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00015c00 + 0x15bdc) != iVar1) {
    __stack_chk_fail();
  }
                    /* WARNING: Could not recover jumptable at 0x00015bee. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (**(code **)(*piVar2 + 0x40))(piVar2);
  return;
}



/* ===== Framework  @ 00015c04 ===== */

/* easel::Framework::Framework(std::function<void ()>) */

void __thiscall easel::Framework::Framework(undefined4 param_1,int *param_2)

{
  undefined4 uVar1;
  int *piVar2;
  int iVar3;
  code *pcVar4;
  int aiStack_30 [4];
  int *local_20;
  int local_18;
  
  local_18 = **(int **)(DAT_00015c94 + 0x15c14);
  uVar1 = pst::core::Framework::GetReference();
  piVar2 = (int *)param_2[4];
  if (piVar2 == (int *)0x0) {
    local_20 = (int *)0x0;
  }
  else if (piVar2 == param_2) {
    local_20 = aiStack_30;
    (**(code **)(*piVar2 + 0xc))();
  }
  else {
    local_20 = (int *)(**(code **)(*piVar2 + 8))();
  }
  iVar3 = pst::core::Framework::StartForApplication(uVar1,aiStack_30,1);
  if (local_20 == aiStack_30) {
    pcVar4 = *(code **)(*local_20 + 0x10);
  }
  else {
    if (local_20 == (int *)0x0) goto LAB_00015c60;
    pcVar4 = *(code **)(*local_20 + 0x14);
  }
  (*pcVar4)();
LAB_00015c60:
  if (iVar3 != 0) {
    pst::log::Print(3,DAT_00015c98 + 0x15c6e,0x10,0,DAT_00015c9c + 0x15c70,DAT_00015ca0 + 0x15c72,
                    iVar3);
  }
  iVar3 = **(int **)(DAT_00015ca4 + 0x15c84) - local_18;
  if (iVar3 == 0) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail(iVar3);
}



/* ===== ~Framework  @ 00015ca8 ===== */

/* easel::Framework::~Framework() */

void __thiscall easel::Framework::~Framework(Framework *this)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00015cdc + 0x15cb6);
  pst::core::Framework::GetReference();
  pst::core::Framework::StopForApplication();
  iVar1 = **(int **)(DAT_00015ce0 + 0x15cca) - iVar1;
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== LifeCycleManager  @ 00015ce4 ===== */

/* easel::LifeCycleManager::LifeCycleManager() */

void __thiscall easel::LifeCycleManager::LifeCycleManager(LifeCycleManager *this)

{
  int iVar1;
  int iVar2;
  
  iVar2 = *(int *)(DAT_00015d24 + 0x15cf4);
  iVar1 = **(int **)(DAT_00015d20 + 0x15cf2);
  *(undefined4 *)(this + 0x20) = 0;
  *(int *)this = iVar2 + 8;
  *(undefined4 *)(this + 4) = 0;
  iVar2 = DAT_00015d28;
  *(undefined4 *)(this + 8) = 0;
  if (**(int **)(iVar2 + 0x15d0e) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== ~LifeCycleManager  @ 00015d2c ===== */

/* easel::LifeCycleManager::~LifeCycleManager() */

void __thiscall easel::LifeCycleManager::~LifeCycleManager(LifeCycleManager *this)

{
  int iVar1;
  LifeCycleManager *pLVar2;
  int *piVar3;
  code *pcVar4;
  
  iVar1 = **(int **)(DAT_00015d8c + 0x15d3c);
  *(int *)this = *(int *)(DAT_00015d90 + 0x15d3e) + 8;
  pLVar2 = *(LifeCycleManager **)(this + 0x20);
  if (pLVar2 == this + 0x10) {
    pcVar4 = *(code **)(*(int *)pLVar2 + 0x10);
  }
  else {
    if (pLVar2 == (LifeCycleManager *)0x0) goto LAB_00015d62;
    pcVar4 = *(code **)(*(int *)pLVar2 + 0x14);
  }
  (*pcVar4)();
LAB_00015d62:
  piVar3 = *(int **)(this + 8);
  *(undefined4 *)(this + 8) = 0;
  if (piVar3 != (int *)0x0) {
    (**(code **)(*piVar3 + 4))();
  }
  iVar1 = **(int **)(DAT_00015d94 + 0x15d7a) - iVar1;
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== ~LifeCycleManager  @ 00015d98 ===== */

/* easel::LifeCycleManager::~LifeCycleManager() */

void __thiscall easel::LifeCycleManager::~LifeCycleManager(LifeCycleManager *this)

{
  int iVar1;
  LifeCycleManager *pLVar2;
  int *piVar3;
  code *pcVar4;
  
  iVar1 = **(int **)(DAT_00015dfc + 0x15da8);
  *(int *)this = *(int *)(DAT_00015e00 + 0x15daa) + 8;
  pLVar2 = *(LifeCycleManager **)(this + 0x20);
  if (pLVar2 == this + 0x10) {
    pcVar4 = *(code **)(*(int *)pLVar2 + 0x10);
  }
  else {
    if (pLVar2 == (LifeCycleManager *)0x0) goto LAB_00015dce;
    pcVar4 = *(code **)(*(int *)pLVar2 + 0x14);
  }
  (*pcVar4)();
LAB_00015dce:
  piVar3 = *(int **)(this + 8);
  *(undefined4 *)(this + 8) = 0;
  if (piVar3 != (int *)0x0) {
    (**(code **)(*piVar3 + 4))();
  }
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00015e04 + 0x15de6) != iVar1) {
    __stack_chk_fail();
  }
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (*(code *)(DAT_00017788 + 0x1778c))(this);
  return;
}



/* ===== Main  @ 00015e08 ===== */

/* easel::LifeCycleManager::Main(std::unique_ptr<easel::ModuleRegistry,
   std::default_delete<easel::ModuleRegistry> >) */

void __thiscall easel::LifeCycleManager::Main(LifeCycleManager *this,undefined4 *param_2)

{
  int *piVar1;
  LifeCycleManager *pLVar2;
  code *pcVar3;
  undefined4 uVar4;
  int iVar5;
  undefined4 *puVar6;
  undefined4 *puVar7;
  Framework aFStack_34 [4];
  int aiStack_30 [4];
  int *local_20;
  int local_18;
  
  local_18 = **(int **)(DAT_00015f58 + 0x15e16);
  uVar4 = *param_2;
  *param_2 = 0;
  piVar1 = *(int **)(this + 8);
  *(undefined4 *)(this + 8) = uVar4;
  if (piVar1 != (int *)0x0) {
    (**(code **)(*piVar1 + 4))();
  }
  pst::log::Print(5,DAT_00015f5c + 0x15e3c,0x5b,0,DAT_00015f60 + 0x15e3e,DAT_00015f64 + 0x15e3a);
  puVar7 = *(undefined4 **)(*(int *)(this + 8) + 8);
  for (puVar6 = *(undefined4 **)(*(int *)(this + 8) + 4); puVar6 != puVar7; puVar6 = puVar6 + 2) {
    (**(code **)(*(int *)*puVar6 + 8))();
  }
  pLVar2 = *(LifeCycleManager **)(this + 0x20);
  if (pLVar2 == (LifeCycleManager *)0x0) {
    local_20 = (int *)0x0;
  }
  else if (pLVar2 == this + 0x10) {
    local_20 = aiStack_30;
    (**(code **)(*(int *)pLVar2 + 0xc))();
  }
  else {
    local_20 = (int *)(**(code **)(*(int *)pLVar2 + 8))();
  }
  Framework::Framework(aFStack_34,aiStack_30);
  if (local_20 == aiStack_30) {
    pcVar3 = *(code **)(*local_20 + 0x10);
  }
  else {
    if (local_20 == (int *)0x0) goto LAB_00015ea2;
    pcVar3 = *(code **)(*local_20 + 0x14);
  }
  (*pcVar3)();
LAB_00015ea2:
  pst::log::Print(5,DAT_00015f68 + 0x15eb0,0x62,0,DAT_00015f6c + 0x15eb2,DAT_00015f70 + 0x15eae);
  puVar7 = *(undefined4 **)(*(int *)(this + 8) + 8);
  for (puVar6 = *(undefined4 **)(*(int *)(this + 8) + 4); puVar6 != puVar7; puVar6 = puVar6 + 2) {
    (**(code **)(*(int *)*puVar6 + 0xc))();
  }
  pst::log::Print(5,DAT_00015f74 + 0x15ee2,0x69,0,DAT_00015f78 + 0x15ee4,DAT_00015f7c + 0x15ee0);
  puVar7 = *(undefined4 **)(*(int *)(this + 8) + 8);
  for (puVar6 = *(undefined4 **)(*(int *)(this + 8) + 4); puVar6 != puVar7; puVar6 = puVar6 + 2) {
    (**(code **)(*(int *)*puVar6 + 0x10))();
  }
  Framework::~Framework(aFStack_34);
  pst::log::Print(5,DAT_00015f84 + 0x15f1c,0x70,0,DAT_00015f80 + 0x15f1a,DAT_00015f88 + 0x15f18);
  iVar5 = *(int *)(this + 8);
  puVar6 = *(undefined4 **)(iVar5 + 8);
  puVar7 = *(undefined4 **)(iVar5 + 4);
  while (puVar6 != puVar7) {
    puVar6 = puVar6 + -2;
    (**(code **)(*(int *)*puVar6 + 0x20))();
    puVar7 = *(undefined4 **)(iVar5 + 4);
  }
  iVar5 = **(int **)(DAT_00015f8c + 0x15f46) - local_18;
  if (iVar5 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar5);
  }
  return;
}



/* ===== ToInitialize  @ 00015f90 ===== */

/* easel::LifeCycleManager::ToInitialize() */

void __thiscall easel::LifeCycleManager::ToInitialize(LifeCycleManager *this)

{
  int iVar1;
  undefined4 *puVar2;
  undefined4 *puVar3;
  
  iVar1 = **(int **)(DAT_00015fec + 0x15fa0);
  pst::log::Print(5,DAT_00015ff8 + 0x15fac,0x5b,0,DAT_00015ff4 + 0x15faa,DAT_00015ff0 + 0x15fa6);
  puVar3 = *(undefined4 **)(*(int *)(this + 8) + 8);
  for (puVar2 = *(undefined4 **)(*(int *)(this + 8) + 4); puVar2 != puVar3; puVar2 = puVar2 + 2) {
    (**(code **)(*(int *)*puVar2 + 8))();
  }
  if (**(int **)(DAT_00015ffc + 0x15fdc) == iVar1) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail();
}



/* ===== ToPostInitialize  @ 00016000 ===== */

/* easel::LifeCycleManager::ToPostInitialize() */

void __thiscall easel::LifeCycleManager::ToPostInitialize(LifeCycleManager *this)

{
  int iVar1;
  undefined4 *puVar2;
  undefined4 *puVar3;
  
  iVar1 = **(int **)(DAT_0001605c + 0x16010);
  pst::log::Print(5,DAT_00016068 + 0x1601c,0x62,0,DAT_00016064 + 0x1601a,DAT_00016060 + 0x16016);
  puVar3 = *(undefined4 **)(*(int *)(this + 8) + 8);
  for (puVar2 = *(undefined4 **)(*(int *)(this + 8) + 4); puVar2 != puVar3; puVar2 = puVar2 + 2) {
    (**(code **)(*(int *)*puVar2 + 0xc))();
  }
  if (**(int **)(DAT_0001606c + 0x1604c) == iVar1) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail();
}



/* ===== ToActivate  @ 00016070 ===== */

/* easel::LifeCycleManager::ToActivate() */

void __thiscall easel::LifeCycleManager::ToActivate(LifeCycleManager *this)

{
  int iVar1;
  undefined4 *puVar2;
  undefined4 *puVar3;
  
  iVar1 = **(int **)(DAT_000160cc + 0x16080);
  pst::log::Print(5,DAT_000160d8 + 0x1608c,0x69,0,DAT_000160d4 + 0x1608a,DAT_000160d0 + 0x16086);
  puVar3 = *(undefined4 **)(*(int *)(this + 8) + 8);
  for (puVar2 = *(undefined4 **)(*(int *)(this + 8) + 4); puVar2 != puVar3; puVar2 = puVar2 + 2) {
    (**(code **)(*(int *)*puVar2 + 0x10))();
  }
  if (**(int **)(DAT_000160dc + 0x160bc) == iVar1) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail();
}



/* ===== ToFinalize  @ 000160e0 ===== */

/* easel::LifeCycleManager::ToFinalize() */

void __thiscall easel::LifeCycleManager::ToFinalize(LifeCycleManager *this)

{
  int iVar1;
  undefined4 *puVar2;
  int iVar3;
  undefined4 *puVar4;
  
  iVar1 = **(int **)(DAT_0001613c + 0x160f0);
  pst::log::Print(5,DAT_00016148 + 0x160fc,0x70,0,DAT_00016144 + 0x160fa,DAT_00016140 + 0x160f6);
  iVar3 = *(int *)(this + 8);
  puVar4 = *(undefined4 **)(iVar3 + 8);
  puVar2 = *(undefined4 **)(iVar3 + 4);
  while (puVar4 != puVar2) {
    puVar4 = puVar4 + -2;
    (**(code **)(*(int *)*puVar4 + 0x20))();
    puVar2 = *(undefined4 **)(iVar3 + 4);
  }
  if (**(int **)(DAT_0001614c + 0x1612c) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== GetModuleRegistry  @ 00016150 ===== */

/* easel::LifeCycleManager::GetModuleRegistry() */

void __thiscall easel::LifeCycleManager::GetModuleRegistry(LifeCycleManager *this)

{
  if (**(int **)(DAT_0001617c + 0x16168) != **(int **)(DAT_00016178 + 0x1615c)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(*(undefined4 *)(this + 8));
  }
  return;
}



/* ===== SetPumpTriggerHandler  @ 00016180 ===== */

/* easel::LifeCycleManager::SetPumpTriggerHandler(std::function<void ()>) */

void __thiscall easel::LifeCycleManager::SetPumpTriggerHandler(LifeCycleManager *this,int *param_2)

{
  int *piVar1;
  code *pcVar2;
  function<void()> afStack_30 [16];
  int *local_20;
  int local_14;
  
  local_20 = (int *)afStack_30;
  local_14 = **(int **)(DAT_000161ec + 0x16192);
  piVar1 = (int *)param_2[4];
  if (piVar1 == (int *)0x0) {
    local_20 = (int *)0x0;
  }
  else if (piVar1 == param_2) {
    (**(code **)(*piVar1 + 0xc))();
  }
  else {
    local_20 = (int *)(**(code **)(*piVar1 + 8))();
  }
  std::function<void()>::swap(afStack_30,this + 0x10);
  if ((function<void()> *)local_20 == afStack_30) {
    pcVar2 = *(code **)(*local_20 + 0x10);
  }
  else {
    if (local_20 == (int *)0x0) goto LAB_000161d4;
    pcVar2 = *(code **)(*local_20 + 0x14);
  }
  (*pcVar2)();
LAB_000161d4:
  if (**(int **)(DAT_000161f0 + 0x161dc) == local_14) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail();
}



/* ===== Inactivate  @ 000161f4 ===== */

/* easel::LifeCycleManager::Inactivate() */

void __thiscall easel::LifeCycleManager::Inactivate(LifeCycleManager *this)

{
  int iVar1;
  code *pcVar2;
  undefined1 auStack_70 [4];
  int local_6c;
  int aiStack_68 [4];
  int *local_58;
  int aiStack_50 [4];
  int *local_40;
  int local_38;
  LifeCycleManager *pLStack_34;
  int *local_28;
  int local_1c;
  
  local_1c = **(int **)(DAT_000162b4 + 0x16208);
  pst::log::Print(5,DAT_000162c0 + 0x16214,0x3d,0,DAT_000162bc + 0x16212,DAT_000162b8 + 0x1620e);
  iVar1 = pst::core::Framework::GetReference();
  local_38 = DAT_000162c4 + 0x1623e;
  local_40 = (int *)0x0;
  local_58 = (int *)0x0;
  pLStack_34 = this;
  local_28 = &local_38;
  pst::core::JobQueue::AddJobFunc
            (auStack_70,*(undefined4 *)(iVar1 + 0x30),&local_38,0,aiStack_50,aiStack_68,0);
  if (local_6c != 0) {
    std::__shared_weak_count::__release_shared();
  }
  if (local_58 == aiStack_68) {
    pcVar2 = *(code **)(*local_58 + 0x10);
LAB_00016272:
    (*pcVar2)();
  }
  else if (local_58 != (int *)0x0) {
    pcVar2 = *(code **)(*local_58 + 0x14);
    goto LAB_00016272;
  }
  if (local_40 == aiStack_50) {
    pcVar2 = *(code **)(*local_40 + 0x10);
LAB_00016286:
    (*pcVar2)();
  }
  else if (local_40 != (int *)0x0) {
    pcVar2 = *(code **)(*local_40 + 0x14);
    goto LAB_00016286;
  }
  if (local_28 == &local_38) {
    pcVar2 = *(code **)(*local_28 + 0x10);
  }
  else {
    if (local_28 == (int *)0x0) goto LAB_0001629c;
    pcVar2 = *(code **)(*local_28 + 0x14);
  }
  (*pcVar2)();
LAB_0001629c:
  if (**(int **)(DAT_000162c8 + 0x162a4) != local_1c) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== AddJobFunc  @ 000162cc ===== */

/* pst::core::JobQueue::AddJobFunc(std::function<void ()>, void const*, std::function<void ()>,
   std::function<void ()>, void const*) */

void pst::core::JobQueue::AddJobFunc
               (int *param_1,int *param_2,int *param_3,undefined4 param_4,int *param_5,int *param_6,
               undefined4 param_7)

{
  FuncJob *pFVar1;
  int *piVar2;
  code *pcVar3;
  int aiStack_70 [4];
  int *local_60;
  int aiStack_58 [4];
  int *local_48;
  int aiStack_40 [4];
  int *local_30;
  int local_28;
  
  local_28 = **(int **)(DAT_000163f4 + 0x162e0);
  pFVar1 = operator_new(0x60);
  piVar2 = (int *)param_3[4];
  if (piVar2 == (int *)0x0) {
    local_30 = (int *)0x0;
  }
  else if (piVar2 == param_3) {
    local_30 = aiStack_40;
    (**(code **)(*piVar2 + 0xc))();
  }
  else {
    local_30 = (int *)(**(code **)(*piVar2 + 8))();
  }
  piVar2 = (int *)param_5[4];
  if (piVar2 == (int *)0x0) {
    local_48 = (int *)0x0;
  }
  else if (piVar2 == param_5) {
    local_48 = aiStack_58;
    (**(code **)(*piVar2 + 0xc))();
  }
  else {
    local_48 = (int *)(**(code **)(*piVar2 + 8))();
  }
  piVar2 = (int *)param_6[4];
  if (piVar2 == (int *)0x0) {
    local_60 = (int *)0x0;
  }
  else if (piVar2 == param_6) {
    local_60 = aiStack_70;
    (**(code **)(*piVar2 + 0xc))();
  }
  else {
    local_60 = (int *)(**(code **)(*piVar2 + 8))();
  }
  FuncJob::FuncJob(pFVar1,aiStack_40,param_4,aiStack_58,aiStack_70,param_7);
  *param_1 = (int)pFVar1;
  piVar2 = operator_new(0x10);
  *piVar2 = *(int *)(DAT_000163f8 + 0x16380) + 8;
  piVar2[1] = 0;
  piVar2[2] = 0;
  piVar2[3] = (int)pFVar1;
  param_1[1] = (int)piVar2;
  if (local_60 == aiStack_70) {
    pcVar3 = *(code **)(*local_60 + 0x10);
LAB_000163a0:
    (*pcVar3)();
  }
  else if (local_60 != (int *)0x0) {
    pcVar3 = *(code **)(*local_60 + 0x14);
    goto LAB_000163a0;
  }
  if (local_48 == aiStack_58) {
    pcVar3 = *(code **)(*local_48 + 0x10);
LAB_000163b4:
    (*pcVar3)();
  }
  else if (local_48 != (int *)0x0) {
    pcVar3 = *(code **)(*local_48 + 0x14);
    goto LAB_000163b4;
  }
  if (local_30 == aiStack_40) {
    pcVar3 = *(code **)(*local_30 + 0x10);
  }
  else {
    if (local_30 == (int *)0x0) goto LAB_000163ca;
    pcVar3 = *(code **)(*local_30 + 0x14);
  }
  (*pcVar3)();
LAB_000163ca:
  if (*param_1 != 0) {
    (**(code **)(*param_2 + 8))(param_2,param_1);
  }
  if (**(int **)(DAT_000163fc + 0x163e2) == local_28) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail();
}



/* ===== OnForeground  @ 00016400 ===== */

/* easel::LifeCycleManager::OnForeground() */

void __thiscall easel::LifeCycleManager::OnForeground(LifeCycleManager *this)

{
  int iVar1;
  undefined4 *puVar2;
  int iVar3;
  undefined4 *puVar4;
  
  iVar1 = **(int **)(DAT_00016460 + 0x16410);
  pst::log::Print(5,DAT_0001646c + 0x1641c,0x45,0,DAT_00016468 + 0x1641a,DAT_00016464 + 0x16416);
  iVar3 = *(int *)(this + 8);
  puVar4 = *(undefined4 **)(iVar3 + 8);
  puVar2 = *(undefined4 **)(iVar3 + 4);
  while (puVar4 != puVar2) {
    puVar4 = puVar4 + -2;
    (**(code **)(*(int *)*puVar4 + 0x14))();
    puVar2 = *(undefined4 **)(iVar3 + 4);
  }
  *(undefined4 *)(this + 4) = 1;
  if (**(int **)(DAT_00016470 + 0x16450) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== OnBackground  @ 00016474 ===== */

/* easel::LifeCycleManager::OnBackground() */

void __thiscall easel::LifeCycleManager::OnBackground(LifeCycleManager *this)

{
  int iVar1;
  undefined4 *puVar2;
  undefined4 *puVar3;
  
  iVar1 = **(int **)(DAT_000164d4 + 0x16484);
  pst::log::Print(5,DAT_000164e0 + 0x16490,0x4d,0,DAT_000164dc + 0x1648e,DAT_000164d8 + 0x1648a);
  puVar3 = *(undefined4 **)(*(int *)(this + 8) + 8);
  for (puVar2 = *(undefined4 **)(*(int *)(this + 8) + 4); puVar2 != puVar3; puVar2 = puVar2 + 2) {
    (**(code **)(*(int *)*puVar2 + 0x18))();
  }
  *(undefined4 *)(this + 4) = 2;
  if (**(int **)(DAT_000164e4 + 0x164c4) == iVar1) {
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail();
}



/* ===== OnTerminate  @ 000164e8 ===== */

/* easel::LifeCycleManager::OnTerminate() */

void __thiscall easel::LifeCycleManager::OnTerminate(LifeCycleManager *this)

{
  int iVar1;
  undefined4 *puVar2;
  int iVar3;
  undefined4 *puVar4;
  int iVar5;
  
  iVar3 = DAT_00016574 + 0x16504;
  iVar5 = DAT_00016570 + 0x16506;
  iVar1 = **(int **)(DAT_00016568 + 0x164fa);
  pst::log::Print(5,iVar3,0x55,0,iVar5,DAT_0001656c + 0x16500);
  pst::log::Print(5,iVar3,0x77,0,iVar5,DAT_00016578 + 0x16522);
  puVar4 = *(undefined4 **)(*(int *)(this + 8) + 4);
  puVar2 = *(undefined4 **)(*(int *)(this + 8) + 8);
  if (puVar4 != puVar2) {
    do {
      iVar3 = (**(code **)(*(int *)*puVar4 + 0x1c))();
      if (puVar2 + -2 == puVar4) break;
      puVar4 = puVar4 + 2;
    } while (iVar3 != 0);
  }
  if (**(int **)(DAT_0001657c + 0x1655a) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== ToInactivate  @ 00016580 ===== */

/* easel::LifeCycleManager::ToInactivate() */

void __thiscall easel::LifeCycleManager::ToInactivate(LifeCycleManager *this)

{
  int iVar1;
  undefined4 *puVar2;
  int iVar3;
  undefined4 *puVar4;
  
  iVar1 = **(int **)(DAT_000165ec + 0x16590);
  pst::log::Print(5,DAT_000165f8 + 0x1659c,0x77,0,DAT_000165f4 + 0x1659a,DAT_000165f0 + 0x16596);
  puVar4 = *(undefined4 **)(*(int *)(this + 8) + 4);
  puVar2 = *(undefined4 **)(*(int *)(this + 8) + 8);
  if (puVar4 != puVar2) {
    do {
      iVar3 = (**(code **)(*(int *)*puVar4 + 0x1c))();
      if (puVar2 + -2 == puVar4) break;
      puVar4 = puVar4 + 2;
    } while (iVar3 != 0);
  }
  if (**(int **)(DAT_000165fc + 0x165dc) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FuncJob  @ 00016600 ===== */

/* pst::core::FuncJob::FuncJob(std::function<void ()>, void const*, std::function<void ()>,
   std::function<void ()>, void const*) */

void __thiscall
pst::core::FuncJob::FuncJob
          (FuncJob *this,int *param_2,undefined4 param_3,int *param_4,int *param_5,
          undefined4 param_6)

{
  int iVar1;
  int *piVar2;
  undefined4 uVar3;
  int iVar4;
  
  iVar4 = *(int *)(DAT_000166c0 + 0x16618);
  iVar1 = **(int **)(DAT_000166bc + 0x16612);
  this[4] = (FuncJob)0x0;
  this[5] = (FuncJob)0x0;
  *(undefined4 *)(this + 8) = 0;
  *(undefined4 *)(this + 0xc) = param_3;
  *(undefined4 *)(this + 0x10) = param_6;
  *(int *)this = iVar4 + 8;
  piVar2 = (int *)param_2[4];
  if (piVar2 == (int *)0x0) {
    *(undefined4 *)(this + 0x28) = 0;
  }
  else if (piVar2 == param_2) {
    *(FuncJob **)(this + 0x28) = this + 0x18;
    (**(code **)(*(int *)param_2[4] + 0xc))((int *)param_2[4],this + 0x18);
  }
  else {
    uVar3 = (**(code **)(*piVar2 + 8))();
    *(undefined4 *)(this + 0x28) = uVar3;
  }
  piVar2 = (int *)param_4[4];
  if (piVar2 == (int *)0x0) {
    uVar3 = 0;
LAB_0001666c:
    *(undefined4 *)(this + 0x40) = uVar3;
  }
  else {
    if (piVar2 != param_4) {
      uVar3 = (**(code **)(*piVar2 + 8))();
      goto LAB_0001666c;
    }
    *(FuncJob **)(this + 0x40) = this + 0x30;
    (**(code **)(*(int *)param_4[4] + 0xc))();
  }
  piVar2 = (int *)param_5[4];
  if (piVar2 == (int *)0x0) {
    uVar3 = 0;
  }
  else {
    if (piVar2 == param_5) {
      *(FuncJob **)(this + 0x58) = this + 0x48;
      (**(code **)(*(int *)param_5[4] + 0xc))();
      goto LAB_000166a2;
    }
    uVar3 = (**(code **)(*piVar2 + 8))();
  }
  *(undefined4 *)(this + 0x58) = uVar3;
LAB_000166a2:
  iVar1 = **(int **)(DAT_000166c4 + 0x166aa) - iVar1;
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== ~FuncJob  @ 000166c8 ===== */

/* pst::core::FuncJob::~FuncJob() */

void __thiscall pst::core::FuncJob::~FuncJob(FuncJob *this)

{
  int iVar1;
  FuncJob *pFVar2;
  code *pcVar3;
  
  iVar1 = **(int **)(DAT_00016748 + 0x166d8);
  *(int *)this = *(int *)(DAT_0001674c + 0x166da) + 8;
  pFVar2 = *(FuncJob **)(this + 0x58);
  if (pFVar2 == this + 0x48) {
    pcVar3 = *(code **)(*(int *)pFVar2 + 0x10);
LAB_000166fc:
    (*pcVar3)();
  }
  else if (pFVar2 != (FuncJob *)0x0) {
    pcVar3 = *(code **)(*(int *)pFVar2 + 0x14);
    goto LAB_000166fc;
  }
  pFVar2 = *(FuncJob **)(this + 0x40);
  if (pFVar2 == this + 0x30) {
    pcVar3 = *(code **)(*(int *)pFVar2 + 0x10);
LAB_00016714:
    (*pcVar3)();
  }
  else if (pFVar2 != (FuncJob *)0x0) {
    pcVar3 = *(code **)(*(int *)pFVar2 + 0x14);
    goto LAB_00016714;
  }
  pFVar2 = *(FuncJob **)(this + 0x28);
  if (pFVar2 == this + 0x18) {
    pcVar3 = *(code **)(*(int *)pFVar2 + 0x10);
  }
  else {
    if (pFVar2 == (FuncJob *)0x0) goto LAB_0001672e;
    pcVar3 = *(code **)(*(int *)pFVar2 + 0x14);
  }
  (*pcVar3)();
LAB_0001672e:
  iVar1 = **(int **)(DAT_00016750 + 0x16736) - iVar1;
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== ~FuncJob  @ 00016754 ===== */

/* pst::core::FuncJob::~FuncJob() */

void __thiscall pst::core::FuncJob::~FuncJob(FuncJob *this)

{
  int iVar1;
  FuncJob *pFVar2;
  code *pcVar3;
  
  iVar1 = **(int **)(DAT_000167d8 + 0x16764);
  *(int *)this = *(int *)(DAT_000167dc + 0x16766) + 8;
  pFVar2 = *(FuncJob **)(this + 0x58);
  if (pFVar2 == this + 0x48) {
    pcVar3 = *(code **)(*(int *)pFVar2 + 0x10);
LAB_00016788:
    (*pcVar3)();
  }
  else if (pFVar2 != (FuncJob *)0x0) {
    pcVar3 = *(code **)(*(int *)pFVar2 + 0x14);
    goto LAB_00016788;
  }
  pFVar2 = *(FuncJob **)(this + 0x40);
  if (pFVar2 == this + 0x30) {
    pcVar3 = *(code **)(*(int *)pFVar2 + 0x10);
LAB_000167a0:
    (*pcVar3)();
  }
  else if (pFVar2 != (FuncJob *)0x0) {
    pcVar3 = *(code **)(*(int *)pFVar2 + 0x14);
    goto LAB_000167a0;
  }
  pFVar2 = *(FuncJob **)(this + 0x28);
  if (pFVar2 == this + 0x18) {
    pcVar3 = *(code **)(*(int *)pFVar2 + 0x10);
  }
  else {
    if (pFVar2 == (FuncJob *)0x0) goto LAB_000167ba;
    pcVar3 = *(code **)(*(int *)pFVar2 + 0x14);
  }
  (*pcVar3)();
LAB_000167ba:
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_000167e0 + 0x167c2) == iVar1) {
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00017788 + 0x1778c))(this);
    return;
  }
  __stack_chk_fail();
}



/* ===== Run  @ 000167e4 ===== */

/* pst::core::FuncJob::Run() */

void __thiscall pst::core::FuncJob::Run(FuncJob *this)

{
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00016818 + 0x167f4) != **(int **)(DAT_00016814 + 0x167f2)) {
    __stack_chk_fail();
  }
                    /* WARNING: Could not recover jumptable at 0x00016812. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (**(code **)(**(int **)(this + 0x28) + 0x18))();
  return;
}



/* ===== OnCanceledBeforeRun  @ 0001681c ===== */

/* pst::core::Job::OnCanceledBeforeRun() */

void pst::core::Job::OnCanceledBeforeRun(void)

{
  if (**(int **)(DAT_00016848 + 0x16834) != **(int **)(DAT_00016844 + 0x16828)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== CancelImpl  @ 0001684c ===== */

/* pst::core::Job::CancelImpl() */

void pst::core::Job::CancelImpl(void)

{
  if (**(int **)(DAT_00016878 + 0x16864) != **(int **)(DAT_00016874 + 0x16858)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== Finish  @ 0001687c ===== */

/* pst::core::FuncJob::Finish() */

void __thiscall pst::core::FuncJob::Finish(FuncJob *this)

{
  code *UNRECOVERED_JUMPTABLE;
  undefined4 *puVar1;
  
  if (this[4] == (FuncJob)0x0) {
    if (*(int **)(this + 0x40) == (int *)0x0) goto LAB_000168bc;
    UNRECOVERED_JUMPTABLE = *(code **)(**(int **)(this + 0x40) + 0x18);
    puVar1 = (undefined4 *)(DAT_000168dc + 0x168ac);
  }
  else {
    if (*(int **)(this + 0x58) == (int *)0x0) {
LAB_000168bc:
      if (**(int **)(DAT_000168e0 + 0x168c4) == **(int **)(DAT_000168d4 + 0x16888)) {
        return;
      }
      goto LAB_000168ce;
    }
    UNRECOVERED_JUMPTABLE = *(code **)(**(int **)(this + 0x58) + 0x18);
    puVar1 = (undefined4 *)(DAT_000168d8 + 0x1689e);
  }
  if (*(int *)*puVar1 == **(int **)(DAT_000168d4 + 0x16888)) {
                    /* WARNING: Could not recover jumptable at 0x000168ba. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*UNRECOVERED_JUMPTABLE)();
    return;
  }
LAB_000168ce:
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail();
}



/* ===== ~__shared_ptr_pointer  @ 000168e4 ===== */

/* std::__shared_ptr_pointer<pst::core::FuncJob*, std::default_delete<pst::core::FuncJob>,
   std::allocator<pst::core::FuncJob> >::~__shared_ptr_pointer() */

void __thiscall
std::
__shared_ptr_pointer<pst::core::FuncJob*,std::default_delete<pst::core::FuncJob>,std::allocator<pst::core::FuncJob>>
::~__shared_ptr_pointer
          (__shared_ptr_pointer<pst::core::FuncJob*,std::default_delete<pst::core::FuncJob>,std::allocator<pst::core::FuncJob>>
           *this)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00016914 + 0x168f0);
  std::__shared_weak_count::~__shared_weak_count((__shared_weak_count *)this);
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00016918 + 0x16900) == iVar1) {
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00017788 + 0x1778c))();
    return;
  }
  __stack_chk_fail();
}



/* ===== __on_zero_shared  @ 0001691c ===== */

/* std::__shared_ptr_pointer<pst::core::FuncJob*, std::default_delete<pst::core::FuncJob>,
   std::allocator<pst::core::FuncJob> >::__on_zero_shared() */

void __thiscall
std::
__shared_ptr_pointer<pst::core::FuncJob*,std::default_delete<pst::core::FuncJob>,std::allocator<pst::core::FuncJob>>
::__on_zero_shared(__shared_ptr_pointer<pst::core::FuncJob*,std::default_delete<pst::core::FuncJob>,std::allocator<pst::core::FuncJob>>
                   *this)

{
  if (*(int **)(this + 0xc) == (int *)0x0) {
    if (**(int **)(DAT_00016968 + 0x16952) == **(int **)(DAT_00016960 + 0x16928)) {
      return;
    }
  }
  else if (**(int **)(DAT_00016964 + 0x16938) == **(int **)(DAT_00016960 + 0x16928)) {
                    /* WARNING: Could not recover jumptable at 0x00016948. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (**(code **)(**(int **)(this + 0xc) + 4))();
    return;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail();
}



/* ===== __on_zero_shared_weak  @ 0001696c ===== */

/* std::__shared_ptr_pointer<pst::core::FuncJob*, std::default_delete<pst::core::FuncJob>,
   std::allocator<pst::core::FuncJob> >::__on_zero_shared_weak() */

void __thiscall
std::
__shared_ptr_pointer<pst::core::FuncJob*,std::default_delete<pst::core::FuncJob>,std::allocator<pst::core::FuncJob>>
::__on_zero_shared_weak
          (__shared_ptr_pointer<pst::core::FuncJob*,std::default_delete<pst::core::FuncJob>,std::allocator<pst::core::FuncJob>>
           *this)

{
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_0001699c + 0x16984) == **(int **)(DAT_00016998 + 0x16978)) {
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00017788 + 0x1778c))(this);
    return;
  }
  __stack_chk_fail();
}



/* ===== FUN_000169a0  @ 000169a0 ===== */

void FUN_000169a0(void)

{
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_000169d0 + 0x169b8) == **(int **)(DAT_000169cc + 0x169ac)) {
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00017788 + 0x1778c))();
    return;
  }
  __stack_chk_fail();
}



/* ===== FUN_000169d4  @ 000169d4 ===== */

void FUN_000169d4(int param_1)

{
  int iVar1;
  int *piVar2;
  
  iVar1 = **(int **)(DAT_00016a10 + 0x169e2);
  piVar2 = operator_new(8);
  *piVar2 = DAT_00016a14 + 0x169fa;
  piVar2[1] = *(int *)(param_1 + 4);
  if (**(int **)(DAT_00016a18 + 0x16a00) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_00016a1c  @ 00016a1c ===== */

void FUN_00016a1c(int param_1,int *param_2)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00016a50 + 0x16a2a);
  *param_2 = DAT_00016a54 + 0x16a34;
  param_2[1] = *(int *)(param_1 + 4);
  if (**(int **)(DAT_00016a58 + 0x16a42) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_00016a5c  @ 00016a5c ===== */

void FUN_00016a5c(void)

{
  if (**(int **)(DAT_00016a88 + 0x16a74) != **(int **)(DAT_00016a84 + 0x16a68)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_00016a8c  @ 00016a8c ===== */

void FUN_00016a8c(void)

{
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00016abc + 0x16aa4) == **(int **)(DAT_00016ab8 + 0x16a98)) {
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00017788 + 0x1778c))();
    return;
  }
  __stack_chk_fail();
}



/* ===== FUN_00016ac0  @ 00016ac0 ===== */

void FUN_00016ac0(int param_1)

{
  undefined4 *puVar1;
  int iVar2;
  int iVar3;
  undefined4 *puVar4;
  
  iVar2 = **(int **)(DAT_00016b2c + 0x16ace);
  iVar3 = *(int *)(param_1 + 4);
  pst::log::Print(5,DAT_00016b34 + 0x16ae2,0x77,0,DAT_00016b30 + 0x16ad0,DAT_00016b38 + 0x16ae0);
  iVar3 = *(int *)(iVar3 + 8);
  puVar4 = *(undefined4 **)(iVar3 + 4);
  puVar1 = *(undefined4 **)(iVar3 + 8);
  if (puVar4 != puVar1) {
    do {
      iVar3 = (**(code **)(*(int *)*puVar4 + 0x1c))();
      if (puVar1 + -2 == puVar4) break;
      puVar4 = puVar4 + 2;
    } while (iVar3 != 0);
  }
  if (**(int **)(DAT_00016b3c + 0x16b1c) != iVar2) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== StartApp  @ 00016b40 ===== */

/* easel::StartApp(std::string const&) */

void easel::StartApp(string *param_1)

{
  int iVar1;
  int iVar2;
  AppManager *this;
  
  iVar1 = **(int **)(DAT_00016bb4 + 0x16b50);
  DataMemoryBarrier(0x1b);
  if (((*(byte *)(DAT_00016bb8 + 0x16b52) & 1) == 0) &&
     (iVar2 = __cxa_guard_acquire(DAT_00016bbc + 0x16b68), iVar2 != 0)) {
    this = (AppManager *)(DAT_00016bc0 + 0x16b72);
    pst::appmanager::AppManager::AppManager(this);
    __cxa_atexit(*(undefined4 *)(DAT_00016bc4 + 0x16b7e),this,
                 *(undefined4 *)(DAT_00016bc8 + 0x16b80));
    __cxa_guard_release(DAT_00016bcc + 0x16b8e);
  }
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00016bd4 + 0x16b98) == iVar1) {
                    /* WARNING: Could not recover jumptable at 0x000177c4. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_000177c8 + 0x177cc))(DAT_00016bd0 + 0x16baa,param_1);
    return;
  }
  __stack_chk_fail();
}



/* ===== GetAppManager  @ 00016bd8 ===== */

/* easel::AppManagerModule::GetAppManager() */

int easel::AppManagerModule::GetAppManager(void)

{
  int iVar1;
  int iVar2;
  AppManager *this;
  
  iVar1 = **(int **)(DAT_00016c40 + 0x16be6);
  DataMemoryBarrier(0x1b);
  if (((*(byte *)(DAT_00016c44 + 0x16be8) & 1) == 0) &&
     (iVar2 = __cxa_guard_acquire(DAT_00016c48 + 0x16bfe), iVar2 != 0)) {
    this = (AppManager *)(DAT_00016c4c + 0x16c08);
    pst::appmanager::AppManager::AppManager(this);
    __cxa_atexit(*(undefined4 *)(DAT_00016c50 + 0x16c14),this,
                 *(undefined4 *)(DAT_00016c54 + 0x16c16));
    __cxa_guard_release(DAT_00016c58 + 0x16c24);
  }
  if (**(int **)(DAT_00016c60 + 0x16c2e) == iVar1) {
    return DAT_00016c5c + 0x16c3a;
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail();
}



/* ===== StartAppWithParam  @ 00016c64 ===== */

/* easel::StartAppWithParam(std::string const&, std::string const&) */

void easel::StartAppWithParam(string *param_1,string *param_2)

{
  int iVar1;
  AppManager *this;
  string asStack_24 [12];
  int local_18;
  
  local_18 = **(int **)(DAT_00016ce8 + 0x16c76);
  DataMemoryBarrier(0x1b);
  if (((*(byte *)(DAT_00016cec + 0x16c78) & 1) == 0) &&
     (iVar1 = __cxa_guard_acquire(DAT_00016cf0 + 0x16c8e), iVar1 != 0)) {
    this = (AppManager *)(DAT_00016cf4 + 0x16c98);
    pst::appmanager::AppManager::AppManager(this);
    __cxa_atexit(*(undefined4 *)(DAT_00016cf8 + 0x16ca4),this,
                 *(undefined4 *)(DAT_00016cfc + 0x16ca6));
    __cxa_guard_release(DAT_00016d00 + 0x16cb4);
  }
  std::string::string(asStack_24,param_2);
  pst::appmanager::AppManager::DoStartAppWithParam((string *)(DAT_00016d04 + 0x16cca),param_1);
  std::string::~string(asStack_24);
  if (**(int **)(DAT_00016d08 + 0x16cda) != local_18) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== AppManagerModule  @ 00016d0c ===== */

/* easel::AppManagerModule::AppManagerModule(int, char**, char const*,
   pst::appmanager::ChangeLifeCycleHandler&) */

void __thiscall
easel::AppManagerModule::AppManagerModule
          (AppManagerModule *this,int param_1,char **param_2,char *param_3,
          ChangeLifeCycleHandler *param_4)

{
  AppManagerModule *pAVar1;
  int iVar2;
  int iVar3;
  
  iVar2 = **(int **)(DAT_00016d9c + 0x16d20);
  *(int *)this = *(int *)(DAT_00016da0 + 0x16d24) + 8;
  *(undefined4 *)(this + 8) = 0;
  *(undefined4 *)(this + 0xc) = 0;
  *(undefined4 *)(this + 4) = 0;
  strlen(param_3);
  std::string::__init((char *)(this + 4),(uint)param_3);
  pAVar1 = this + 0x10;
  *(undefined4 *)pAVar1 = 0;
  *(undefined4 *)(this + 0x14) = 0;
  *(undefined4 *)(this + 0x18) = 0;
  *(ChangeLifeCycleHandler **)(this + 0x1c) = param_4;
  if (0 < param_1) {
    iVar3 = 0;
    do {
      if (0 < iVar3) {
        std::string::push_back((char)pAVar1);
      }
      std::string::append((char *)pAVar1);
      iVar3 = iVar3 + 1;
    } while (param_1 != iVar3);
  }
  iVar2 = **(int **)(DAT_00016da4 + 0x16d88) - iVar2;
  if (iVar2 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar2);
  }
  return;
}



/* ===== OnInitialize  @ 00016da8 ===== */

/* easel::AppManagerModule::OnInitialize() */

void easel::AppManagerModule::OnInitialize(void)

{
  if (**(int **)(DAT_00016dd4 + 0x16dc0) != **(int **)(DAT_00016dd0 + 0x16db4)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== OnActivate  @ 00016dd8 ===== */

/* easel::AppManagerModule::OnActivate() */

void __thiscall easel::AppManagerModule::OnActivate(AppManagerModule *this)

{
  int iVar1;
  undefined4 uVar2;
  code *pcVar3;
  AppManager *this_00;
  undefined1 auStack_68 [4];
  int local_64;
  int aiStack_60 [4];
  int *local_50;
  int aiStack_48 [4];
  int *local_38;
  int local_30;
  AppManagerModule *local_2c;
  int *local_20;
  int local_18;
  
  local_18 = **(int **)(DAT_00016ec0 + 0x16de8);
  DataMemoryBarrier(0x1b);
  if (((*(byte *)(DAT_00016ec4 + 0x16dea) & 1) == 0) &&
     (iVar1 = __cxa_guard_acquire(DAT_00016ec8 + 0x16e00), iVar1 != 0)) {
    this_00 = (AppManager *)(DAT_00016ecc + 0x16e0a);
    pst::appmanager::AppManager::AppManager(this_00);
    __cxa_atexit(*(undefined4 *)(DAT_00016ed0 + 0x16e16),this_00,
                 *(undefined4 *)(DAT_00016ed4 + 0x16e18));
    __cxa_guard_release(DAT_00016ed8 + 0x16e26);
  }
  iVar1 = pst::core::Framework::GetReference();
  uVar2 = *(undefined4 *)(iVar1 + 0x30);
  local_30 = DAT_00016edc + 0x16e40;
  *(int *)(((uint)&local_30 | 4) + 4) = DAT_00016ee0 + 0x16e46;
  local_38 = (int *)0x0;
  local_50 = (int *)0x0;
  local_2c = this;
  local_20 = &local_30;
  pst::core::JobQueue::AddJobFunc(auStack_68,uVar2,&local_30,0,aiStack_48,aiStack_60,0);
  if (local_64 != 0) {
    std::__shared_weak_count::__release_shared();
  }
  if (local_50 == aiStack_60) {
    pcVar3 = *(code **)(*local_50 + 0x10);
LAB_00016e7e:
    (*pcVar3)();
  }
  else if (local_50 != (int *)0x0) {
    pcVar3 = *(code **)(*local_50 + 0x14);
    goto LAB_00016e7e;
  }
  if (local_38 == aiStack_48) {
    pcVar3 = *(code **)(*local_38 + 0x10);
LAB_00016e92:
    (*pcVar3)();
  }
  else if (local_38 != (int *)0x0) {
    pcVar3 = *(code **)(*local_38 + 0x14);
    goto LAB_00016e92;
  }
  if (local_20 == &local_30) {
    pcVar3 = *(code **)(*local_20 + 0x10);
  }
  else {
    if (local_20 == (int *)0x0) goto LAB_00016ea8;
    pcVar3 = *(code **)(*local_20 + 0x14);
  }
  (*pcVar3)();
LAB_00016ea8:
  if (**(int **)(DAT_00016ee4 + 0x16eb0) != local_18) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== OnInactivate  @ 00016ee8 ===== */

/* easel::AppManagerModule::OnInactivate() */

void easel::AppManagerModule::OnInactivate(void)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00016f14 + 0x16f00) - **(int **)(DAT_00016f10 + 0x16ef4);
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== OnFinalize  @ 00016f18 ===== */

/* easel::AppManagerModule::OnFinalize() */

void easel::AppManagerModule::OnFinalize(void)

{
  if (**(int **)(DAT_00016f44 + 94000) != **(int **)(DAT_00016f40 + 0x16f24)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== Exit  @ 00016f48 ===== */

/* easel::AppManagerModule::Exit() */

void easel::AppManagerModule::Exit(void)

{
  int in_r0;
  int iVar1;
  int iVar2;
  AppManager *this;
  
  iVar1 = **(int **)(DAT_00016fbc + 0x16f58);
  DataMemoryBarrier(0x1b);
  if (((*(byte *)(DAT_00016fc0 + 0x16f5a) & 1) == 0) &&
     (iVar2 = __cxa_guard_acquire(DAT_00016fc4 + 0x16f70), iVar2 != 0)) {
    this = (AppManager *)(DAT_00016fc8 + 0x16f7a);
    pst::appmanager::AppManager::AppManager(this);
    __cxa_atexit(*(undefined4 *)(DAT_00016fcc + 0x16f86),this,
                 *(undefined4 *)(DAT_00016fd0 + 0x16f88));
    __cxa_guard_release(DAT_00016fd4 + 0x16f96);
  }
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_00016fdc + 0x16fa0) == iVar1) {
                    /* WARNING: Could not recover jumptable at 0x000177d4. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(_fini + DAT_000177d8))(DAT_00016fd8 + 0x16fb2,in_r0 + 4);
    return;
  }
  __stack_chk_fail();
}



/* ===== GetAppParam  @ 00016fe0 ===== */

/* easel::AppManagerModule::GetAppParam() */

void __thiscall easel::AppManagerModule::GetAppParam(AppManagerModule *this)

{
  if (**(int **)(DAT_0001700c + 0x16ff8) != **(int **)(DAT_00017008 + 0x16fec)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(this);
  }
  return;
}



/* ===== ~AppManagerModule  @ 00017010 ===== */

/* easel::AppManagerModule::~AppManagerModule() */

void __thiscall easel::AppManagerModule::~AppManagerModule(AppManagerModule *this)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00017058 + 0x17022);
  *(int *)this = *(int *)(DAT_0001705c + 0x17024) + 8;
  std::string::~string((string *)(this + 0x10));
  std::string::~string((string *)(this + 4));
  iVar1 = **(int **)(DAT_00017060 + 0x17048) - iVar1;
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== ~AppManagerModule  @ 00017064 ===== */

/* easel::AppManagerModule::~AppManagerModule() */

void __thiscall easel::AppManagerModule::~AppManagerModule(AppManagerModule *this)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_000170b4 + 0x17076);
  *(int *)this = *(int *)(DAT_000170b8 + 0x17078) + 8;
  std::string::~string((string *)(this + 0x10));
  std::string::~string((string *)(this + 4));
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_000170bc + 0x1709c) == iVar1) {
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00017788 + 0x1778c))(this);
    return;
  }
  __stack_chk_fail();
}



/* ===== OnPostInitialize  @ 000170c0 ===== */

/* easel::ModuleBase<easel::AppManagerModule>::OnPostInitialize() */

void easel::ModuleBase<easel::AppManagerModule>::OnPostInitialize(void)

{
  if (**(int **)(DAT_000170ec + 0x170d8) != **(int **)(DAT_000170e8 + 0x170cc)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== OnForeground  @ 000170f0 ===== */

/* easel::ModuleBase<easel::AppManagerModule>::OnForeground() */

void easel::ModuleBase<easel::AppManagerModule>::OnForeground(void)

{
  if (**(int **)(DAT_0001711c + 0x17108) != **(int **)(DAT_00017118 + 0x170fc)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== OnBackground  @ 00017120 ===== */

/* easel::ModuleBase<easel::AppManagerModule>::OnBackground() */

void easel::ModuleBase<easel::AppManagerModule>::OnBackground(void)

{
  if (**(int **)(DAT_0001714c + 0x17138) != **(int **)(DAT_00017148 + 0x1712c)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== GetClassId  @ 00017150 ===== */

/* easel::ModuleBase<easel::AppManagerModule>::GetClassId() */

undefined4 easel::ModuleBase<easel::AppManagerModule>::GetClassId(void)

{
  if (**(int **)(DAT_00017184 + 0x17168) == **(int **)(DAT_0001717c + 0x1715c)) {
    return *(undefined4 *)(DAT_00017180 + 0x17174);
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail();
}



/* ===== FUN_00017188  @ 00017188 ===== */

void FUN_00017188(void)

{
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_000171b8 + 0x171a0) == **(int **)(DAT_000171b4 + 0x17194)) {
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00017788 + 0x1778c))();
    return;
  }
  __stack_chk_fail();
}



/* ===== FUN_000171bc  @ 000171bc ===== */

void FUN_000171bc(int param_1)

{
  int iVar1;
  int *piVar2;
  int iVar3;
  
  iVar1 = **(int **)(DAT_000171fc + 0x171ca);
  piVar2 = operator_new(0xc);
  *piVar2 = DAT_00017200 + 0x171e2;
  iVar3 = *(int *)(param_1 + 8);
  piVar2[1] = *(int *)(param_1 + 4);
  piVar2[2] = iVar3;
  if (**(int **)(DAT_00017204 + 0x171ec) != iVar1) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_00017208  @ 00017208 ===== */

void FUN_00017208(int param_1,int *param_2)

{
  int iVar1;
  int iVar2;
  
  iVar2 = **(int **)(DAT_00017240 + 0x17216);
  *param_2 = DAT_00017244 + 0x17220;
  iVar1 = *(int *)(param_1 + 8);
  param_2[1] = *(int *)(param_1 + 4);
  param_2[2] = iVar1;
  if (**(int **)(DAT_00017248 + 0x17232) != iVar2) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_0001724c  @ 0001724c ===== */

void FUN_0001724c(void)

{
  if (**(int **)(DAT_00017278 + 0x17264) != **(int **)(DAT_00017274 + 0x17258)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== FUN_0001727c  @ 0001727c ===== */

void FUN_0001727c(void)

{
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_000172ac + 0x17294) == **(int **)(DAT_000172a8 + 0x17288)) {
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)(DAT_00017788 + 0x1778c))();
    return;
  }
  __stack_chk_fail();
}



/* ===== FUN_000172b0  @ 000172b0 ===== */

void FUN_000172b0(int param_1)

{
  undefined4 uVar1;
  int iVar2;
  string asStack_24 [12];
  int local_18;
  
  local_18 = **(int **)(DAT_000172f8 + 0x172be);
  iVar2 = *(int *)(param_1 + 4);
  uVar1 = *(undefined4 *)(param_1 + 8);
  std::string::string(asStack_24,(string *)(iVar2 + 4));
  pst::appmanager::AppManager::Initialize
            (uVar1,asStack_24,*(undefined4 *)(iVar2 + 0x1c),iVar2 + 0x10);
  std::string::~string(asStack_24);
  if (**(int **)(DAT_000172fc + 0x172ea) != local_18) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== ModuleRegistry  @ 00017300 ===== */

/* easel::ModuleRegistry::ModuleRegistry() */

void __thiscall easel::ModuleRegistry::ModuleRegistry(ModuleRegistry *this)

{
  int iVar1;
  
  iVar1 = **(int **)(DAT_00017348 + 0x17310);
  *(int *)this = *(int *)(DAT_0001734c + 0x17312) + 8;
  *(undefined4 *)(this + 4) = 0;
  *(undefined4 *)(this + 8) = 0;
  *(undefined4 *)(this + 0xc) = 0;
  std::vector<easel::ModuleRegistry::Entry,std::allocator<easel::ModuleRegistry::Entry>>::reserve
            ((vector<easel::ModuleRegistry::Entry,std::allocator<easel::ModuleRegistry::Entry>> *)
             (this + 4),4);
  iVar1 = **(int **)(DAT_00017350 + 0x17338) - iVar1;
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== reserve  @ 00017354 ===== */

/* std::vector<easel::ModuleRegistry::Entry, std::allocator<easel::ModuleRegistry::Entry>
   >::reserve(unsigned int) */

void __thiscall
std::vector<easel::ModuleRegistry::Entry,std::allocator<easel::ModuleRegistry::Entry>>::reserve
          (vector<easel::ModuleRegistry::Entry,std::allocator<easel::ModuleRegistry::Entry>> *this,
          uint param_1)

{
  void *pvVar1;
  undefined4 uVar2;
  size_t __n;
  void *__src;
  FILE *__stream;
  int iVar3;
  void *__dest;
  void *pvVar4;
  int local_28 [2];
  int local_20;
  
  local_20 = **(int **)(DAT_0001741c + 0x17366);
  __src = *(void **)this;
  if ((uint)(*(int *)(this + 8) - (int)__src >> 3) < param_1) {
    iVar3 = *(int *)(this + 4);
    if (param_1 == 0) {
      pvVar1 = (void *)0x0;
    }
    else {
      if (0x1fffffff < param_1) {
        std::logic_error::logic_error((logic_error *)local_28,(char *)(DAT_00017420 + 0x173f4));
        local_28[0] = *(int *)(DAT_00017428 + 0x17400) + 8;
        __stream = (FILE *)**(undefined4 **)(DAT_00017424 + 0x173fe);
        uVar2 = std::logic_error::what();
        fprintf(__stream,(char *)(DAT_0001742c + 0x17416),uVar2);
                    /* WARNING: Subroutine does not return */
        abort();
      }
      pvVar1 = operator_new(param_1 << 3);
    }
    __n = iVar3 - (int)__src;
    pvVar4 = (void *)((int)pvVar1 + ((int)__n >> 3) * 8);
    __dest = (void *)((int)pvVar4 - __n);
    if (0 < (int)__n) {
      memcpy(__dest,__src,__n);
      __src = *(void **)this;
    }
    *(void **)this = __dest;
    *(void **)(this + 4) = pvVar4;
    *(void **)(this + 8) = (void *)((int)pvVar1 + param_1 * 8);
    if (__src != (void *)0x0) {
      iVar3 = **(int **)(DAT_00017430 + 0x173c0) - local_20;
      if (iVar3 == 0) {
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
        (*(code *)(DAT_00017788 + 0x1778c))();
        return;
      }
      goto LAB_000173e8;
    }
  }
  iVar3 = **(int **)(DAT_00017434 + 0x173dc) - local_20;
  if (iVar3 == 0) {
    return;
  }
LAB_000173e8:
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail(iVar3);
}



/* ===== ~ModuleRegistry  @ 00017438 ===== */

/* easel::ModuleRegistry::~ModuleRegistry() */

void __thiscall easel::ModuleRegistry::~ModuleRegistry(ModuleRegistry *this)

{
  int iVar1;
  int *piVar2;
  int *piVar3;
  
  iVar1 = **(int **)(DAT_000174ac + 0x17448);
  *(int *)this = *(int *)(DAT_000174b0 + 0x1744a) + 8;
  piVar2 = *(int **)(this + 4);
  piVar3 = *(int **)(this + 8);
  if (piVar2 != piVar3) {
    do {
      if ((int *)*piVar2 != (int *)0x0) {
        (**(code **)(*(int *)*piVar2 + 4))();
      }
      piVar2 = piVar2 + 2;
    } while (piVar3 != piVar2);
    piVar2 = *(int **)(this + 4);
  }
  if (piVar2 != (int *)0x0) {
    piVar3 = *(int **)(this + 8);
    if (piVar3 != piVar2) {
      *(uint *)(this + 8) = (int)piVar3 + (~((int)piVar3 + (-8 - (int)piVar2)) & 0xfffffff8U);
    }
    operator_delete(piVar2);
  }
  iVar1 = **(int **)(DAT_000174b4 + 0x1749c) - iVar1;
  if (iVar1 != 0) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1);
  }
  return;
}



/* ===== ~ModuleRegistry  @ 000174b8 ===== */

/* easel::ModuleRegistry::~ModuleRegistry() */

void __thiscall easel::ModuleRegistry::~ModuleRegistry(ModuleRegistry *this)

{
  int iVar1;
  int *piVar2;
  int *piVar3;
  
  iVar1 = **(int **)(DAT_00017534 + 0x174c8);
  *(int *)this = *(int *)(DAT_00017538 + 0x174ca) + 8;
  piVar2 = *(int **)(this + 4);
  piVar3 = *(int **)(this + 8);
  if (piVar2 != piVar3) {
    do {
      if ((int *)*piVar2 != (int *)0x0) {
        (**(code **)(*(int *)*piVar2 + 4))();
      }
      piVar2 = piVar2 + 2;
    } while (piVar3 != piVar2);
    piVar2 = *(int **)(this + 4);
  }
  if (piVar2 != (int *)0x0) {
    piVar3 = *(int **)(this + 8);
    if (piVar3 != piVar2) {
      *(uint *)(this + 8) = (int)piVar3 + (~((int)piVar3 + (-8 - (int)piVar2)) & 0xfffffff8U);
    }
    operator_delete(piVar2);
  }
                    /* WARNING: Subroutine does not return */
  if (**(int **)(DAT_0001753c + 0x1751c) != iVar1) {
    __stack_chk_fail();
  }
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (*(code *)(DAT_00017788 + 0x1778c))(this);
  return;
}



/* ===== Register  @ 00017540 ===== */

/* easel::ModuleRegistry::Register(std::unique_ptr<easel::ModuleBaseInterface,
   std::default_delete<easel::ModuleBaseInterface> >) */

void __thiscall easel::ModuleRegistry::Register(ModuleRegistry *this,undefined4 *param_2)

{
  undefined4 *puVar1;
  undefined4 local_20;
  undefined4 uStack_1c;
  int local_14;
  
  local_14 = **(int **)(DAT_0001759c + 0x17550);
  uStack_1c = (**(code **)(*(int *)*param_2 + 0x24))();
  local_20 = *param_2;
  *param_2 = 0;
  puVar1 = *(undefined4 **)(this + 8);
  if (puVar1 < *(undefined4 **)(this + 0xc)) {
    *puVar1 = local_20;
    puVar1[1] = uStack_1c;
    *(int *)(this + 8) = *(int *)(this + 8) + 8;
  }
  else {
    std::vector<easel::ModuleRegistry::Entry,std::allocator<easel::ModuleRegistry::Entry>>::
    __push_back_slow_path<easel::ModuleRegistry::Entry>
              ((vector<easel::ModuleRegistry::Entry,std::allocator<easel::ModuleRegistry::Entry>> *)
               (this + 4),(Entry *)&local_20);
  }
  if (**(int **)(DAT_000175a0 + 0x1758e) != local_14) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail();
  }
  return;
}



/* ===== IsRegistered  @ 000175a4 ===== */

/* easel::ModuleRegistry::IsRegistered(unsigned int) */

void __thiscall easel::ModuleRegistry::IsRegistered(ModuleRegistry *this,uint param_1)

{
  int iVar1;
  int iVar2;
  bool bVar3;
  
  iVar1 = *(int *)(this + 4);
  iVar2 = *(int *)(this + 8);
  if (iVar1 != iVar2) {
    do {
      bVar3 = *(uint *)(iVar1 + 4) != param_1;
      if (bVar3) {
        iVar1 = iVar1 + 8;
      }
    } while (bVar3 && iVar2 != iVar1);
  }
  if (**(int **)(DAT_000175ec + 0x175d2) != **(int **)(DAT_000175e8 + 0x175b0)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(iVar1 != iVar2);
  }
  return;
}



/* ===== DoGetModule  @ 000175f0 ===== */

/* easel::ModuleRegistry::DoGetModule(unsigned int) const */

void __thiscall easel::ModuleRegistry::DoGetModule(ModuleRegistry *this,uint param_1)

{
  uint *puVar1;
  undefined4 *puVar2;
  undefined4 uVar3;
  undefined4 *puVar4;
  
  puVar4 = *(undefined4 **)(this + 4);
  puVar2 = *(undefined4 **)(this + 8);
  if (puVar4 != puVar2) {
    do {
      puVar1 = puVar4 + 1;
      if (*puVar1 != param_1) {
        puVar4 = puVar4 + 2;
      }
    } while (*puVar1 != param_1 && puVar2 != puVar4);
  }
  if (puVar4 == puVar2) {
    uVar3 = 0;
  }
  else {
    uVar3 = *puVar4;
  }
  if (**(int **)(DAT_00017638 + 0x17624) != **(int **)(DAT_00017634 + 0x175fc)) {
                    /* WARNING: Subroutine does not return */
    __stack_chk_fail(uVar3);
  }
  return;
}



/* ===== __push_back_slow_path<easel::ModuleRegistry::Entry>  @ 0001763c ===== */

/* void std::vector<easel::ModuleRegistry::Entry, std::allocator<easel::ModuleRegistry::Entry>
   >::__push_back_slow_path<easel::ModuleRegistry::Entry>(easel::ModuleRegistry::Entry&&) */

void __thiscall
std::vector<easel::ModuleRegistry::Entry,std::allocator<easel::ModuleRegistry::Entry>>::
__push_back_slow_path<easel::ModuleRegistry::Entry>
          (vector<easel::ModuleRegistry::Entry,std::allocator<easel::ModuleRegistry::Entry>> *this,
          Entry *param_1)

{
  undefined4 uVar1;
  void *pvVar2;
  int iVar3;
  size_t __n;
  FILE *__stream;
  void *__src;
  uint uVar4;
  void *__dest;
  int iVar5;
  uint uVar6;
  int local_2c [2];
  int local_24;
  
  local_24 = **(int **)(DAT_00017750 + 0x17650);
  __src = *(void **)this;
  uVar6 = (*(int *)(this + 4) - (int)__src >> 3) + 1;
  if (0x1fffffff < uVar6) {
    std::__vector_base_common<true>::__throw_length_error();
    __src = *(void **)this;
  }
  if ((uint)(*(int *)(this + 8) - (int)__src >> 3) < 0xfffffff) {
    iVar5 = *(int *)(this + 4);
    uVar4 = *(int *)(this + 8) - (int)__src >> 2;
    if (uVar4 < uVar6) {
      uVar4 = uVar6;
    }
    iVar3 = iVar5 - (int)__src >> 3;
    if (uVar4 == 0) {
      uVar4 = 0;
      pvVar2 = (void *)0x0;
      goto LAB_000176e4;
    }
    if (0x1fffffff < uVar4) {
      std::logic_error::logic_error((logic_error *)local_2c,(char *)(DAT_00017754 + 0x176a0));
      local_2c[0] = *(int *)(DAT_0001775c + 0x176ac) + 8;
      __stream = (FILE *)**(undefined4 **)(DAT_00017758 + 0x176aa);
      uVar1 = std::logic_error::what();
      fprintf(__stream,(char *)(DAT_00017760 + 0x176c2),uVar1);
                    /* WARNING: Subroutine does not return */
      abort();
    }
  }
  else {
    iVar5 = *(int *)(this + 4);
    uVar4 = 0x1fffffff;
    iVar3 = iVar5 - (int)__src >> 3;
  }
  pvVar2 = operator_new(uVar4 << 3);
LAB_000176e4:
  uVar1 = *(undefined4 *)(param_1 + 4);
  *(undefined4 *)((int)pvVar2 + iVar3 * 8) = *(undefined4 *)param_1;
  *(undefined4 *)((int)pvVar2 + iVar3 * 8 + 4) = uVar1;
  __n = iVar5 - (int)__src;
  __dest = (void *)((int)pvVar2 + (iVar3 * 8 - __n));
  if (0 < (int)__n) {
    memcpy(__dest,__src,__n);
    __src = *(void **)this;
  }
  *(void **)this = __dest;
  *(int *)(this + 4) = (int)pvVar2 + iVar3 * 8 + 8;
  *(void **)(this + 8) = (void *)((int)pvVar2 + uVar4 * 8);
  if (__src == (void *)0x0) {
    iVar3 = **(int **)(DAT_00017768 + 0x17740) - local_24;
    if (iVar3 == 0) {
      return;
    }
  }
  else {
    iVar3 = **(int **)(DAT_00017764 + 0x17724) - local_24;
    if (iVar3 == 0) {
                    /* WARNING: Could not recover jumptable at 0x00017784. Too many branches */
                    /* WARNING: Treating indirect jump as call */
      (*(code *)(DAT_00017788 + 0x1778c))();
      return;
    }
  }
                    /* WARNING: Subroutine does not return */
  __stack_chk_fail(iVar3);
}



/* ===== _fini  @ 000177dc ===== */

void _fini(void)

{
  return;
}



/* ===== __aeabi_unwind_cpp_pr0  @ 0001b000 ===== */

/* WARNING: Control flow encountered bad instruction data */

void __aeabi_unwind_cpp_pr0(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== __gmon_start__  @ 0001b004 ===== */

/* WARNING: Control flow encountered bad instruction data */

void __gmon_start__(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== __cxa_finalize  @ 0001b008 ===== */

/* WARNING: Control flow encountered bad instruction data */

void __cxa_finalize(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== _ITM_deregisterTMCloneTable  @ 0001b00c ===== */

/* WARNING: Control flow encountered bad instruction data */

void _ITM_deregisterTMCloneTable(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== _ITM_registerTMCloneTable  @ 0001b010 ===== */

/* WARNING: Control flow encountered bad instruction data */

void _ITM_registerTMCloneTable(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== _Jv_RegisterClasses  @ 0001b014 ===== */

/* WARNING: Control flow encountered bad instruction data */

void _Jv_RegisterClasses(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== abort  @ 0001b018 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void abort(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== __stack_chk_fail  @ 0001b01c ===== */

/* WARNING: Control flow encountered bad instruction data */

void __stack_chk_fail(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== __aeabi_unwind_cpp_pr1  @ 0001b024 ===== */

/* WARNING: Control flow encountered bad instruction data */

void __aeabi_unwind_cpp_pr1(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== strlen  @ 0001b02c ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

size_t strlen(char *__s)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== memcpy  @ 0001b030 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void * memcpy(void *__dest,void *__src,size_t __n)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== operator.delete  @ 0001b034 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void operator_delete(void *param_1)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== __cxa_guard_acquire  @ 0001b038 ===== */

/* WARNING: Control flow encountered bad instruction data */

void __cxa_guard_acquire(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== operator.new  @ 0001b03c ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void * operator_new(uint param_1)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== __cxa_pure_virtual  @ 0001b040 ===== */

/* WARNING: Control flow encountered bad instruction data */

void __cxa_pure_virtual(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== __cxa_guard_release  @ 0001b044 ===== */

/* WARNING: Control flow encountered bad instruction data */

void __cxa_guard_release(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== StopTimeoutCheck  @ 0001b048 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void pst::appmanager::AppManager::Counter::StopTimeoutCheck(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== Print  @ 0001b04c ===== */

/* WARNING: Control flow encountered bad instruction data */

void pst::log::Print(void...)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== GetReference  @ 0001b050 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void pst::core::Framework::GetReference(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== UpdateCurrentContextHangTimeout  @ 0001b054 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void pst::core::Framework::UpdateCurrentContextHangTimeout(ulonglong param_1)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== ParseResumeFactorString  @ 0001b058 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void pst::services::binder::ServiceBase::ParseResumeFactorString
               (string *param_1,string *param_2,string *param_3)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== lock  @ 0001b05c ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void std::mutex::lock(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== unlock  @ 0001b060 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void std::mutex::unlock(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== _exit  @ 0001b064 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void _exit(int __status)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== execlp  @ 0001b068 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

int execlp(char *__file,char *__arg,...)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== execve  @ 0001b06c ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

int execve(char *__path,char **__argv,char **__envp)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== fork  @ 0001b070 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

__pid_t fork(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== waitpid  @ 0001b074 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

__pid_t waitpid(__pid_t __pid,int *__stat_loc,int __options)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== StopForApplication  @ 0001b078 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void pst::core::Framework::StopForApplication(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== StartForApplication  @ 0001b07c ===== */

/* WARNING: Control flow encountered bad instruction data */

void pst::core::Framework::StartForApplication(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== __get_deleter  @ 0001b080 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void std::__shared_weak_count::__get_deleter(type_info *param_1)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== __release_shared  @ 0001b084 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void std::__shared_weak_count::__release_shared(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== ~__shared_weak_count  @ 0001b088 ===== */

/* WARNING: Control flow encountered bad instruction data */

void __thiscall std::__shared_weak_count::~__shared_weak_count(__shared_weak_count *this)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== Initialize  @ 0001b08c ===== */

/* WARNING: Control flow encountered bad instruction data */

void pst::appmanager::AppManager::Initialize(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== TerminateApp  @ 0001b090 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void pst::appmanager::AppManager::TerminateApp(string *param_1)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== DoStartAppWithParam  @ 0001b094 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void pst::appmanager::AppManager::DoStartAppWithParam(string *param_1,string *param_2)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== StartApp  @ 0001b098 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void pst::appmanager::AppManager::StartApp(string *param_1)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== AppManager  @ 0001b09c ===== */

/* WARNING: Control flow encountered bad instruction data */

void __thiscall pst::appmanager::AppManager::AppManager(AppManager *this)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== ~AppManager  @ 0001b0a0 ===== */

/* WARNING: Control flow encountered bad instruction data */

void __thiscall pst::appmanager::AppManager::~AppManager(AppManager *this)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== __init  @ 0001b0a4 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void std::string::__init(char *param_1,uint param_2)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== append  @ 0001b0a8 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void std::string::append(char *param_1)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== push_back  @ 0001b0ac ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void std::string::push_back(char param_1)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== string  @ 0001b0b0 ===== */

/* WARNING: Control flow encountered bad instruction data */

void __thiscall std::string::string(string *this,string *param_1)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== ~string  @ 0001b0b4 ===== */

/* WARNING: Control flow encountered bad instruction data */

void __thiscall std::string::~string(string *this)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== __cxa_atexit  @ 0001b0b8 ===== */

/* WARNING: Control flow encountered bad instruction data */

void __cxa_atexit(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== fprintf  @ 0001b0bc ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

int fprintf(FILE *__stream,char *__format,...)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== what  @ 0001b0c0 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void std::logic_error::what(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== __throw_length_error  @ 0001b0c4 ===== */

/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Unknown calling convention -- yet parameter storage is locked */

void std::__vector_base_common<true>::__throw_length_error(void)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}



/* ===== logic_error  @ 0001b0c8 ===== */

/* WARNING: Control flow encountered bad instruction data */

void __thiscall std::logic_error::logic_error(logic_error *this,char *param_1)

{
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}


