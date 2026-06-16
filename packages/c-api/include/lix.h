#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

typedef struct LixSession LixSession;

typedef struct Test1Value {
  uint32_t value1;
  bool value2;
} Test1Value;

uint32_t lix_version(void);

struct LixSession *open(void);

bool close(struct LixSession *ptr);

uintptr_t get_active_branch(struct LixSession *ptr, uint8_t *out_buf, uintptr_t buf_len);

uintptr_t Test1(struct LixSession *ptr, uint8_t *out_buf, uintptr_t buf_len, uintptr_t limit);

uintptr_t create_branch(struct LixSession *ptr,
                        const uint8_t *name_ptr,
                        uintptr_t name_len,
                        uint8_t *out_buf,
                        uintptr_t buf_len);
