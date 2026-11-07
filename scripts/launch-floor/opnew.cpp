// Linked into the floor's `opnew` variant: the process-wide operator new and
// delete replacements that ThorVG's tvgInitializer.cpp defines, so the floor
// shows what replacing them costs at launch.
#include <cstdlib>
#include <new>

void *operator new(std::size_t size) { return std::malloc(size); }
void operator delete(void *ptr) noexcept { std::free(ptr); }
void *operator new[](std::size_t size) { return std::malloc(size); }
void operator delete[](void *ptr) noexcept { std::free(ptr); }
