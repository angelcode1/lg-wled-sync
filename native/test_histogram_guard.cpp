#include <cassert>
#include <cstring>
#include <stdexcept>
#include <cstddef>
extern "C" {
typedef bool (*P)();
void* lg_histogram_init_guarded(void* (*)(P), P, char*, std::size_t) noexcept;
int lg_histogram_preset_guarded(int (*)(void*, int), void*, int, char*, std::size_t) noexcept;
int lg_histogram_get_guarded(int (*)(void*, void*), void*, void*, char*, std::size_t) noexcept;
int lg_histogram_finish_guarded(int (*)(void*), void*, char*, std::size_t) noexcept;
}
bool yes() {return true;}
void* throwing_init(P) {throw std::runtime_error("LSRegisterPubPriv FAILED");}
void* good_init(P) {return reinterpret_cast<void*>(0x10);}
int throwing_preset(void*, int) {throw std::runtime_error("preset failed");}
int good_preset(void*, int) {return 0;}
int throwing_get(void*, void*) {throw std::runtime_error("sample failed");}
int throwing_finish(void*) {throw std::runtime_error("finish failed");}
int good_finish(void*) {return 0;}
int main() {
  char error[96]{};
  assert(lg_histogram_init_guarded(throwing_init,yes,error,sizeof(error))==nullptr);
  assert(std::strcmp(error,"LSRegisterPubPriv FAILED")==0);
  assert(lg_histogram_init_guarded(good_init,yes,error,sizeof(error))!=nullptr);
  assert(error[0]=='\0');
  assert(lg_histogram_preset_guarded(throwing_preset,nullptr,0,error,sizeof(error))==-1);
  assert(std::strcmp(error,"preset failed")==0);
  assert(lg_histogram_preset_guarded(good_preset,nullptr,0,error,sizeof(error))==0);
  assert(lg_histogram_get_guarded(throwing_get,nullptr,error,error,sizeof(error))==-1);
  assert(std::strcmp(error,"sample failed")==0);
  assert(lg_histogram_finish_guarded(throwing_finish,nullptr,error,sizeof(error))==-1);
  assert(std::strcmp(error,"finish failed")==0);
  assert(lg_histogram_finish_guarded(good_finish,nullptr,error,sizeof(error))==0);
}
