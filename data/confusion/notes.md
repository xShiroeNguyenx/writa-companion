# Confusion-set tiếng Việt — sổ tay curate

> **Trạng thái:** bản khởi tạo do Claude ghi từ hiểu biết sẵn có. Chưa qua kiểm định
> corpus. Nguyễn Khánh bổ sung/sửa trực tiếp vào file này; P1 sẽ chuyển thành
> `data/confusion/*.toml` để engine đọc.

## Vì sao file này quan trọng hơn nó trông

L1 (tập âm tiết) bắt được lỗi *non-word* với precision 100% — nhưng nó **mù hoàn toàn**
với lỗi *real-word*: `chia sẽ`, `sữa lỗi`, `xử dụng` đều gồm các âm tiết hợp lệ. Đó là
loại lỗi người Việt mắc nhiều nhất, và cách duy nhất bắt được là **sinh candidate từ
bảng dưới đây rồi để n-gram LM phán quyết**.

Nói cách khác: chất lượng của Writa bị chặn trên bởi độ đầy đủ của file này. Không
phải bởi model.

## Ký hiệu

| Ký hiệu | Nghĩa |
|---|---|
| `A` ✗ → `B` ✓ | A luôn sai, B luôn đúng. Một chiều. Đủ điều kiện auto-fix nếu L1 cũng loại A. |
| `A` ⇄ `B` | Cả hai đều là từ thật, **phân biệt bằng ngữ cảnh**. LM quyết định. Không bao giờ auto-fix. |
| `A` = `B` | Hai biến thể **đều đúng**. Tuyệt đối KHÔNG báo lỗi. |
| ⚠️ | Tôi không chắc chắn — cần Nguyễn Khánh xác minh trước khi đưa vào engine. |

---

## 0. Biến thể đều đúng — KHÔNG ĐƯỢC BÁO LỖI

Mục này đặt đầu tiên vì nó là nguồn false-positive nguy hiểm nhất. Mỗi dòng ở đây
mà engine báo lỗi là một lần user mất niềm tin.

**Vị trí dấu thanh** (đã xử lý tự động trong `phonology.rs`):
```
hòa = hoà      khỏe = khoẻ     thúy = thuý     xòe = xoè
quý = qúy      hủy = huỷ       tòa = toà       lóe = loé
```

**i / y** — cả hai đều được chấp nhận:
```
kỹ = kĩ        lý = lí         mỹ = mĩ         quý = quí
hy = hi        sỹ = sĩ         tỷ = tỉ         vy = vi
kỳ = kì        ly = li         mỳ = mì
```
Lưu ý: `bác sĩ`, `công ty`, `quy định` là dạng thông dụng nhất — nhưng dạng kia
không sai, chỉ là lựa chọn quy chuẩn khác.

**Cặp từ khác**:
```
cảm ơn = cám ơn          hàng ngày = hằng ngày
dòng = giòng ⚠️           nghìn = ngàn (khác vùng miền, đều đúng)
sáng lạn ⚠️               trăm phần trăm
```

---

## 1. Thanh HỎI ↔ NGÃ — lỗi số 1

Đây là lỗi phổ biến nhất và cũng là lỗi ít tool nào xử lý. Chiếm phần lớn giá trị
của Writa.

### Luật hài hoà thanh điệu trong từ láy

Mẹo nhớ truyền thống:
- **"Chị Huyền mang nặng ngã đau"** — trong từ láy, nếu tiếng kia mang thanh
  **huyền / nặng** (hoặc chính nó là ngã) → tiếng này mang **NGÃ**.
- **"Anh Sắc không hỏi một tiếng"** — nếu tiếng kia mang thanh
  **sắc / ngang (không dấu)** → tiếng này mang **HỎI**.

Ví dụ áp luật: `lẽ ra` (ra = ngang → lẻ? không, đây không phải từ láy).
Luật chỉ áp cho **từ láy**, không áp cho từ ghép — đây là lý do không thể chỉ dùng
luật mà phải có LM.

Ví dụ đúng của luật: `mờ mịt`, `lã chã`, `dõng dạc`, `bẽ bàng` (ngã + huyền/nặng);
`nhỏ nhắn`, `hớn hở`, `thẳng thắn` (hỏi + sắc/ngang).

### Cặp từ thật ⇄ từ thật — cần ngữ cảnh

Đây là nhóm quan trọng nhất: cả hai đều là từ có nghĩa, LM phải chọn.

| Hỏi | Ngã | Phân biệt |
|---|---|---|
| `sẻ` | `sẽ` | chia sẻ, chim sẻ ⇄ sẽ làm (tương lai) |
| `vẻ` | `vẽ` | vẻ đẹp, vẻ mặt ⇄ vẽ tranh |
| `lẻ` | `lẽ` | số lẻ, lẻ loi ⇄ lẽ ra, có lẽ, lẽ phải |
| `sửa` | `sữa` | sửa chữa, sửa lỗi ⇄ sữa tươi, sữa mẹ |
| `nghỉ` | `nghĩ` | nghỉ ngơi, nghỉ phép ⇄ suy nghĩ, nghĩ rằng |
| `bảo` | `bão` | bảo vệ, bảo đảm ⇄ cơn bão |
| `hải` | `hãi` | hải sản, hàng hải ⇄ kinh hãi, sợ hãi |
| `nổi` | `nỗi` | nổi tiếng, nổi bật ⇄ nỗi buồn, nỗi lo |
| `ngủ` | `ngũ` | đi ngủ, giấc ngủ ⇄ ngũ cốc, đội ngũ, số ngũ |
| `vỏ` | `võ` | vỏ cây, vỏ trứng ⇄ võ thuật, học võ |
| `mảnh` | `mãnh` | mảnh vỡ, mảnh đất ⇄ mãnh liệt, dũng mãnh |
| `mẩu` | `mẫu` | mẩu giấy, mẩu bánh ⇄ mẫu số, mẫu giáo, người mẫu |
| `bẩy` | `bẫy` | bẩy lên (đòn bẩy) ⇄ cái bẫy, bẫy chuột |
| `lở` | `lỡ` | sạt lở, lở đất ⇄ lỡ hẹn, lỡ tay, lỡ dở |
| `dở` | `dỡ` | dở dang, dở tệ ⇄ dỡ hàng, tháo dỡ |
| `đả` | `đã` | đả kích, đấu đả ⇄ đã làm (quá khứ) |
| `vẩn` | `vẫn` | vẩn đục, lẩn vẩn ⇄ vẫn còn |
| `lẩn` | `lẫn` | lẩn trốn, lẩn tránh ⇄ lẫn nhau, lẫn lộn |
| `kẻ` | `kẽ` | kẻ trộm, kẻ bảng ⇄ kẽ hở, khe kẽ |
| `quảng` | `quãng` | quảng cáo, Quảng Nam ⇄ quãng đường, khoảng quãng |
| `xả` | `xã` | xả rác, xả nước ⇄ xã hội, xã viên |
| `tả` | `tã` | miêu tả, phía tả ⇄ tã lót |
| `giả` | `giã` | hàng giả, giả sử ⇄ giã gạo, giã từ |
| `cử` | `cữ` | cử chỉ, cử động ⇄ cữ sữa, ở cữ ⚠️ |
| `rưởi` | `rưỡi` | (ít dùng) ⇄ hai giờ rưỡi, một triệu rưỡi |
| `bả` | `bã` | bả vai ⇄ bã đậu, bã mía |
| `chả` | `chã` | chả lụa, chả lẽ ⇄ lã chã |
| `hỏng` | `hõng` | hỏng hóc ⇄ (hầu như không dùng) |
| `sảy` | `sẫy` | sảy thai, xảy ra ⇄ ⚠️ |

### Từ chỉ có một dạng đúng — thường bị viết sai thanh

**Đúng là NGÃ** (hay bị viết thành hỏi):
```
cũng          mãi mãi       dữ liệu       giữ gìn       sẵn sàng
suy nghĩ      nỗ lực        cũ            những        vẫn
lãng mạn      dĩ nhiên      ngẫu nhiên    xã hội        mỹ thuật
kỹ thuật      diễn ra       miễn phí      tiễn biệt     lũ lụt
cưỡng chế     dãy núi       mãnh liệt     đã            nghĩa
trĩu          rõ ràng ⚠️     hãy           mỗi           chữa bệnh
cữu ⚠️         nghĩ          dũng cảm      bãi biển      vũ trụ
```

**Đúng là HỎI** (hay bị viết thành ngã):
```
chia sẻ       nghỉ ngơi     xảy ra        hiểu          chuẩn
tưởng tượng   rảnh          củng cố       chỉnh sửa     kỷ niệm
đủ            cẩn thận      chuyển        kiểm tra      phát triển
hủy           bẩn           chẳng         hẳn           điển hình
hiển nhiên    quả           bảo đảm       khỏi          thoải mái
sở hữu        thủ tục       tỉnh          bổ sung       ảnh hưởng
khẩn cấp      tổng hợp      chủ yếu       để ý          mở
```

**Cặp cực dễ nhầm cần nhớ riêng:**
- `kỷ niệm` (HỎI) nhưng `kỹ thuật` (NGÃ) — cùng âm "ky", khác thanh, khác nghĩa
- `sửa chữa` — HỎI rồi NGÃ, trong cùng một từ
- `rảnh rỗi` — HỎI rồi NGÃ
- `chia sẻ` (HỎI) nhưng `sẽ đến` (NGÃ)
- `nghỉ ngơi` (HỎI) nhưng `suy nghĩ` (NGÃ)

---

## 2. Âm đầu S / X

| Chỉ `s` | Chỉ `x` | Cặp ⇄ cần ngữ cảnh |
|---|---|---|
| sản xuất, sạch sẽ, sâu sắc | xin lỗi, xuống, xanh | `sử dụng` ⇄ `xử lý` |
| lịch sử, sự việc, sinh sản | xã hội, xuất phát, xem | `suất ăn`/`công suất` ⇄ `xuất phát`/`xuất sắc` |
| sắp xếp (cả s và x) | xuề xoà, xóm | `sót` (bỏ sót) ⇄ `xót` (thương xót) |
| số, sáng, sớm | xây dựng, xưa | `sương` (sương mù) ⇄ `xương` (xương cốt) |
| sẵn, sức | xúc động, xưởng | `sẻ` (chia sẻ) ⇄ `xẻ` (mổ xẻ, xẻ gỗ) |
| | | `sai` ⇄ `xai` (không có — chỉ `sai`) |

**Lỗi một chiều rất phổ biến:**
```
xử dụng      ✗ → sử dụng      ✓
sử lý        ✗ → xử lý        ✓
suất sắc     ✗ → xuất sắc     ✓
sơ xuất      ✗ → sơ suất      ✓
bổ xung      ✗ → bổ sung      ✓
sáng lạng    ✗ → xán lạn      ✓
đường xá     ✗ → đường sá     ✓
cọ sát       ✗ → cọ xát       ✓
xúc tích     ✗ → súc tích     ✓
hàm xúc      ✗ → hàm súc      ✓
xoay sở      ✗ → xoay xở      ✓
sáng sủa     ✓ (không phải lỗi)
```

---

## 3. Âm đầu CH / TR

| Cặp ⇄ | Phân biệt |
|---|---|
| `chuyện` ⇄ `truyện` | câu chuyện, chuyện gì ⇄ quyển truyện, truyện ngắn |
| `chung` ⇄ `trung` | chung thủy, nói chung ⇄ trung thành, trung tâm |
| `chí` ⇄ `trí` | ý chí, chí hướng ⇄ trí tuệ, trí thức |
| `chở` ⇄ `trở` | chở hàng, chuyên chở ⇄ trở về, trở thành |
| `chải` ⇄ `trải` | chải đầu, chải chuốt ⇄ trải nghiệm, trải qua |
| `chồng` ⇄ `trồng` | người chồng, chồng chất ⇄ trồng cây |
| `chật` ⇄ `trật` | chật chội, chật hẹp ⇄ trật tự, trật khớp |
| `chưng` ⇄ `trưng` | bánh chưng, chưng cất ⇄ trưng bày, trưng cầu |
| `cháo` ⇄ `tráo` | ăn cháo ⇄ tráo trở, đánh tráo |
| `chân` ⇄ `trân` | chân thành, bàn chân ⇄ trân trọng, trân quý |
| `chăn` ⇄ `trăn` | chăn nuôi, cái chăn ⇄ con trăn, trăn trở |
| `che` ⇄ `tre` | che chở ⇄ cây tre |
| `chống` ⇄ `trống` | chống lại ⇄ cái trống, trống rỗng |
| `chèo` ⇄ `trèo` | chèo thuyền, hát chèo ⇄ trèo cây |

**Lỗi một chiều:**
```
tựu chung    ✗ → tựu trung    ✓  (tóm lại, nhìn chung)
vô hình chung ✗ → vô hình trung ✓
chưng cầu    ✗ → trưng cầu    ✓
chau chuốt   ✗ → trau chuốt   ✓
chính chắn   ✗ → chín chắn    ✓
chưởng ⚠️
```

---

## 4. Âm đầu R / D / GI

| Cặp ⇄ | Phân biệt |
|---|---|
| `dành` ⇄ `giành` | để dành, dành dụm ⇄ giành giật, giành chiến thắng |
| `dấu` ⇄ `giấu` | dấu vết, dấu câu ⇄ che giấu, giấu đồ |
| `dây` ⇄ `giây` | dây điện, sợi dây ⇄ giây phút, mấy giây |
| `dở` ⇄ `giở` | dở dang, dở tệ ⇄ giở sách, giở trò |
| `dục` ⇄ `giục` | giáo dục, thể dục ⇄ thúc giục, đốc giục |
| `da` ⇄ `gia` ⇄ `ra` | da thịt, da bò ⇄ gia đình, tham gia ⇄ đi ra |
| `dì` ⇄ `gì` | cô dì ⇄ cái gì |
| `dòng` ⇄ `giòng` | dòng sông, dòng chữ ⇄ (biến thể cũ, ít dùng) |
| `rơi` ⇄ `dơi` | rơi xuống ⇄ con dơi |
| `rành` ⇄ `dành` | rành mạch, rành rẽ ⇄ để dành |
| `rẽ` ⇄ `dẽ` | rẽ phải, chia rẽ ⇄ (không dùng) |
| `giặt` ⇄ `dặt` | giặt quần áo ⇄ (không dùng) |
| `rác` ⇄ `giác` | rác thải ⇄ cảm giác, giác quan |
| `dạy` ⇄ `giạy` ⇄ `rạy` | dạy học ⇄ (chỉ `dạy` đúng) |

**Lỗi một chiều:**
```
giành dụm    ✗ → dành dụm     ✓
dành chiến thắng ✗ → giành chiến thắng ✓
che dấu      ✗ → che giấu     ✓
dấu tên      ✗ → giấu tên     ✓  (nhưng `dấu tên` đúng nếu nghĩa "ký hiệu tên") ⚠️
```

---

## 5. Âm đầu L / N (lỗi phát âm miền Bắc)

| Cặp ⇄ | Phân biệt |
|---|---|
| `lên` ⇄ `nên` | đi lên, lên xe ⇄ nên làm, vì thế nên |
| `lo` ⇄ `no` | lo lắng, lo sợ ⇄ ăn no, no bụng |
| `lửa` ⇄ `nửa` | ngọn lửa ⇄ một nửa, nửa đêm |
| `lăm` ⇄ `năm` | mười lăm ⇄ năm tháng, năm 2026 |
| `lâu` ⇄ `nâu` | lâu dài ⇄ màu nâu |
| `lấy` ⇄ `nấy` | lấy đồ ⇄ ai nấy |
| `lão` ⇄ `não` | lão hoá ⇄ bộ não, não nề |
| `lam` ⇄ `nam` | màu lam ⇄ miền Nam, nam giới |
| `lung` ⇄ `nung` | lung linh ⇄ nung chảy |

Chỉ có `l`: `làm`, `lời`, `luôn`, `lại`, `lúc`, `lần`.
Chỉ có `n`: `này`, `nào`, `nếu`, `nhưng` (nh), `nói`.

---

## 6. Âm cuối N / NG và T / C (lỗi phát âm miền Nam)

### n ⇄ ng

| Cặp | Phân biệt |
|---|---|
| `hoàn` ⇄ `hoàng` | hoàn thành, hoàn toàn ⇄ hoàng hôn, Hoàng gia |
| `bàn` ⇄ `bàng` | cái bàn, bàn luận ⇄ bàng hoàng, cây bàng |
| `quan` ⇄ `quang` | quan tâm, quan hệ ⇄ ánh quang, Quang |
| `tan` ⇄ `tang` | tan học, tan biến ⇄ đám tang, tang lễ |
| `lan` ⇄ `lang` | hoa lan, lan toả ⇄ lang thang, khoai lang |
| `chán` ⇄ `chang` | chán nản ⇄ chang chang |
| `tin` ⇄ `ting` | tin tức ⇄ (không có) |
| `nhân` ⇄ `nhâng` | nhân viên ⇄ nhâng nháo |

**Lỗi một chiều kinh điển:**
```
bàng quang   ✗ → bàng quan    ✓  (bàng quan = không quan tâm;
                                    bàng quang = bọng đái — hai từ khác nghĩa!)
```

### t ⇄ c

| Cặp | Phân biệt |
|---|---|
| `việt` ⇄ `việc` | Việt Nam ⇄ công việc |
| `bắt` ⇄ `bắc` | bắt đầu, bắt tay ⇄ phương Bắc, Bắc Kinh |
| `tất` ⇄ `tấc` | tất cả, đôi tất ⇄ một tấc đất |
| `lượt` ⇄ `lược` | lần lượt, đợt lượt ⇄ lược sử, sơ lược, cái lược |
| `mát` ⇄ `mác` | mát mẻ ⇄ nhãn mác |
| `hát` ⇄ `hác` | ca hát ⇄ (không có) |
| `thuật` ⇄ `thuộc` | kỹ thuật ⇄ thuộc về |
| `bất` ⇄ `bấc` | bất ngờ ⇄ gió bấc, bấc đèn |

---

## 7. Cặp từ dễ nhầm về NGHĨA (không phải lỗi chính tả thuần)

Nhóm này engine chỉ nên **gợi ý nhẹ**, không auto-fix, vì cả hai đều đúng chính tả
và người viết có thể chủ ý.

| Cặp | Phân biệt |
|---|---|
| `tri thức` ⇄ `trí thức` | tri thức = kiến thức ⇄ trí thức = người có học |
| `giả thiết` ⇄ `giả thuyết` | giả thiết = điều cho trước (toán) ⇄ giả thuyết = phỏng đoán khoa học |
| `bản` ⇄ `bảng` | bản sao, văn bản ⇄ bảng biểu, bảng đen |
| `chuẩn` ⇄ `chẩn` | chuẩn mực, tiêu chuẩn ⇄ chẩn đoán, chẩn bệnh |
| `khai trương` ⇄ `khai chương` | chỉ `khai trương` đúng |
| `yếu điểm` ⇄ `điểm yếu` | ⚠️ `yếu điểm` gốc Hán-Việt = điểm quan trọng, nhưng thực tế bị dùng như "điểm yếu". Cần quyết định có báo hay không. |
| `cứu cánh` | ⚠️ nghĩa gốc = mục đích cuối cùng, không phải "vị cứu tinh". Bị dùng sai rất rộng. |

---

## 8. Từ sai phổ biến — một chiều, đủ điều kiện auto-fix nếu L1 cũng loại

```
nghành       ✗ → ngành        ✓   (L1 bắt được: ngh + a không hợp lệ)
ngiên cứu    ✗ → nghiên cứu   ✓   (L1 bắt được: ng + i không hợp lệ)
xử lí        = xử lý              (biến thể i/y, KHÔNG phải lỗi)
chuẩn đoán   ✗ → chẩn đoán    ✓
thăm quan    ✗ → tham quan    ✓
nhận chức    ✗ → nhậm chức    ✓
sát nhập     ✗ → sáp nhập     ✓
lãng mạng    ✗ → lãng mạn     ✓
suông sẻ     ✗ → suôn sẻ      ✓
lập lại      ✗ → lặp lại      ✓   (lặp = repeat; lập = establish)
nề nếp       ✗ → nền nếp      ✓   ⚠️ (một số từ điển nhận cả hai)
rốt cục      ✗ → rốt cuộc     ✓   ⚠️
phong thinh  ✗ → phong thanh  ✓
đề bạc       ✗ → đề bạt       ✓
tựu chung    ✗ → tựu trung    ✓
tham quan    ✓
xán lạn      ✓
```

---

## 9. Việc còn phải làm

- [ ] Nguyễn Khánh review, đặc biệt các dòng ⚠️
- [ ] Bổ sung từ vựng chuyên ngành hay gặp trong công việc (rivercrane, thuật ngữ IT)
- [ ] Đối chiếu tần suất thực từ viwiki: cặp nào gần như không xuất hiện thì bỏ,
      cặp nào xuất hiện nhiều thì ưu tiên
- [ ] Chuyển sang `data/confusion/*.toml` (P1, tách theo lớp lỗi)
- [ ] Với mỗi cặp ⇄, thu ví dụ câu thật từ corpus để làm eval set
- [ ] Mục tiêu P1: 200 cặp đã kiểm định. Mục tiêu v1: 500–1000 cặp.
