# Game Sample — เกมตัวอย่างของ ket-rs

เกม 2D arena ขนาดเล็ก (Bevy) ที่ขับเคลื่อนด้วยเอนจินตัดสินใจ **ket**
อ้างอิง: [katgpt-rs](https://github.com/lidm0707/katgpt-rs) — โปรเจกต์ต้นทาง
ที่ให้ `katgpt-core` (salience gate, urgency probe bank, smooth-min router)
และ `katgpt-sense` (`SectorProjection`) ซึ่ง ket-rs นำมาห่อไว้

โค้ด: `examples/game_sample.rs` — รันด้วย
`cargo run --example game_sample`

## 1. เกมเล่นอย่างไร

| องค์ประกอบ | บทบาท |
| --- | --- |
| **Player (น้ำเงิน)** | ตัวเรา เดินด้วย **WASD / ลูกศร** มีแรงเฉื่อย (เร่งค่อยๆ หยุดค่อยๆ) |
| **Enemy (แดง)** | นักล่าที่ขับด้วย AI — **ถามเอนจิน ket ทุกเฟรม** ว่าจะทำอะไร (Chase / Wander / Flee / Orbit / Evade) มี **HP 10** (หลอดเหนือหัว) ที่ลดตามเวลา — *เวลา = เลือด* |
| **เหรียญ (เหลือง)** | กับดักเคลื่อนที่ 6 อัน เด้งไปมา **เก็บให้ครบ 6 = ชนะ** ไม่หายเอง |
| **HP ของ player** | 3 หัวใจ (หลอดเหนือหัว) enemy ชน = −1 พร้อมกันภัย 1.5 วิ (ตัวกระพริบ) หมด = แพ้ |

**ชนะ:** เก็บเหรียญครบ 6 → `YOU WIN`
**แพ้:** HP player เหลือ 0 → `YOU LOSE`

**ปุ่มควบคุม**

| ปุ่ม | ทำอะไร |
| --- | --- |
| `WASD` / ลูกศร | เดิน (ได้ทั่วจอ เคลื่อนที่แบบนุ่ม) |
| `R` | เริ่มเกมใหม่ (หลังชนะ/แพ้) |
| `Esc` | รีสตาร์ททันที ได้ทุกเมื่อ |

**ระบบเสริม**

- **เวลา = เลือด enemy:** เลือดลด 0.25/วิ กระพริบทุกครั้งที่หัก หมดแล้ว respawn เต็ม
- **เหรียญทำร้าย enemy:** ชนเหรียญ = enemy เสีย 1 HP (cooldown 1 วิ) พร้อม push-out แบบนุ่ม — ไม่วาร์ป แค่ค่อยๆ ถูกแยกออกโดยยังเดินตามทิศที่ AI คำนวณไว้
- **สปรินต์สุดชีวิต:** enemy เลือดยิ่งน้อยยิ่งเร็ว (สูงสุด 2 เท่า)
- **กราฟการเคลื่อนที่:** จุด trail จางๆ ตามรอย enemy

## 2. ใช้ katgpt (ket) อย่างไร

ไม่ต้องเทรน ไม่มี if/else เขียน AI เอง — ไปป์ไลน์คือ
**กฎ → latent → การตัดสินใจ**:

### ขั้นที่ 1 — กฎเขียน evidence ลง latent

ทุกเฟรม `encode_state` เขียน 6 มิติ (`STATE_DIM = 6`):

| Dim | ความหมาย | กฎ |
| --- | --- | --- |
| 0 `threat` | 2.0 ถ้า player อยู่ใน 240 px | อันตรายใกล้ |
| 1 `hurt` | 1.5 ถ้า HP player ≤ 1 **หรือ** HP enemy < 30% | สุดชีวิต |
| 2 `cornered` | 2.0 ถ้าโดนไล่ติมุม/ขอบ | ต้องขัง |
| 3 `loot` | 1.0 มีเหรียญใกล้ + สูงสุด 2.0 ตามความเร็วพุ่งเข้า, +2.0 ถ้าเหรียญพุ่งใส่ใน 150 px | ภัยเข้าประชิด |
| 4 `coin_dx` | ทิศ x (signed) ไปเหรียญใกล้สุด (ใน 260 px) | แบริ่ง |
| 5 `coin_dy` | ทิศ y ของเดียวกัน | แบริ่ง |

`KetConflictDetector` ตรวจ finite ทุก dim ก่อนใช้งาน

### ขั้นที่ 2 — เอนจิน plan + decide

`KetQuery` ถามด้วย `TypedQuestion::PickWeighted` 5 ตัวเลือก พร้อม
**direction bank ต่อ choice** (`ACTION_DIRECTIONS`) — แต่ละ action อ่าน
latent ผ่านโปรไฟล์ของตัวเอง:

| Action | แถว (dim0..5) | ชนะเมื่อ |
| --- | --- | --- |
| `Chase` | `[0.9, 0, -0.3, -0.05, 0, 0]` | มี threat ไม่มีภัย — ไล่ player |
| `Wander` | `[0,0,0,0,0,0]` | สถานะกลางๆ |
| `Flee` | `[0.9, 0.6, 0.6, -0.5, 0, 0]` | โดนไล่ + เจ็บ/ต้องขัง — หนี |
| `Orbit` | `[-0.4, -0.4, -0.4, -0.3, 1.2, 1.2]` | เหรียญนิ่งใกล้ๆ — เดินอ้อม |
| `Evade` | `[0, 0, 0, 1.0, 0, 0]` | เหรียญพุ่งเข้าใกล้ใน 150 px — หลบ |

สูตรให้คะแนนต่อ choice (ใน `decision.rs`):
`0.5 × logistic(domain_score) + 0.5 × logistic(dot(dir_c, dims))` —
argmax ชนะ เปลี่ยน latent = เปลี่ยน action โดยตรง
`domain_score` มาจาก `SectorProjection` ternary ต้นน้ำ (sector Safety /
Time / Resource / General — sector Resource push ด้วยทิศเหรียญ) และ
**urgency probe bank** ติดแท็ก Routine / Elevated / Critical ซึ่งคูณความดุ
ของ enemy (×1.0 / 1.25 / 1.6)

### ขั้นที่ 3 — เคลื่อนที่ตามการตัดสินใจ

แต่ละ action แปลงเป็นทิศ: Chase = ตรงเข้า player, Flee = หนีออก (+ clamp
วงแหวน), Orbit = เดินสัมผัสรอบเหรียญใกล้สุด, Evade = วิ่งออกจากตำแหน่ง
เหรียญที่พยากรณ์ 1.3 วิข้างหน้า (+ แรงดึงเข้ากลาง กันติดมุม), Wander =
หัวหมุนวน

### ทำไมออกแบบแบบนี้

AI ทั้งตัวคือ **ข้อมูลใน latent + แถว direction** — เพิ่มพฤติกรรมใหม่ =
เพิ่ม 1 แถว ไม่ต้องเขียน branch เพิ่ม state 6 ตัวเลขชุดเดียวขับเคลื่อน
scoring, urgency และ gating ผ่าน primitive ของ katgpt-rs ทั้งหมด
(sigmoid dot-product ไม่ใช่ softmax, hot path ไม่ allocate)
