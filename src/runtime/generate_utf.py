#!/usr/bin/env python3
"""Generate portable UTF vector IR from the retained scalar reference."""
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parent
PATTERN = re.compile(r"define internal [^\n]+ @([\w.]+)\([^\n]*\) nounwind \{\n.*?\n\}\n", re.S)


def constant(n, bits, value):
    return "<" + ", ".join(f"i{bits} {value}" for _ in range(n)) + ">"


def shuffle(n, bits, indexes):
    mask = "<" + ", ".join(f"i32 {i}" for i in indexes) + ">"
    return f"shufflevector <{n} x i{bits}> %v, <{n} x i{bits}> zeroinitializer, <{len(indexes)} x i32> {mask}"


def ascii_copy(utf8, width=16, suffix=""):
    ty, srcbits, dstbits = ("string", 8, 16) if utf8 else ("utf8string", 16, 8)
    cast = "zext" if utf8 else "trunc"
    return f"""define internal %tz.{ty} @tz.{ty}.from_ascii{suffix}(ptr %source, i64 %length) nounwind {{
entry:
  %result = call %tz.{ty} @tz.{ty}.allocate(i64 %length)
  %data = extractvalue %tz.{ty} %result, 0
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %vn, %vector ], [ %sn, %scalar ]
  %remaining = sub i64 %length, %i
  %enough = icmp uge i64 %remaining, {width}
  br i1 %enough, label %vector, label %tail
vector:
  %vp = getelementptr i{srcbits}, ptr %source, i64 %i
  %v = load <{width} x i{srcbits}>, ptr %vp, align {srcbits // 8}
  %converted = {cast} <{width} x i{srcbits}> %v to <{width} x i{dstbits}>
  %vq = getelementptr i{dstbits}, ptr %data, i64 %i
  store <{width} x i{dstbits}> %converted, ptr %vq, align {dstbits // 8}
  %vn = add i64 %i, {width}
  br label %loop
tail:
  %done = icmp eq i64 %i, %length
  br i1 %done, label %exit, label %scalar
scalar:
  %p = getelementptr i{srcbits}, ptr %source, i64 %i
  %a = load i{srcbits}, ptr %p, align {srcbits // 8}
  %b = {cast} i{srcbits} %a to i{dstbits}
  %q = getelementptr i{dstbits}, ptr %data, i64 %i
  store i{dstbits} %b, ptr %q, align {dstbits // 8}
  %sn = add i64 %i, 1
  br label %loop
exit:
  ret %tz.{ty} %result
}}
"""


def compare_vector(body, bits):
    width = 16 // (bits // 8)
    equality = ".equal(" in body
    byte_compare = not equality and bits == 8
    loop = "test" if byte_compare else "loop"
    a, b = ("%left_data", "%right_data") if byte_compare else ("%a", "%b")
    limit = "%an" if equality else "%length"
    count = width * 2
    body = body.replace(f"label %{loop}", "label %vector_loop", 1)
    body = body.replace("[ 0, %entry ]", "[ %vi, %vector_loop ]" +
                        ("" if equality else ", [ %vi, %vector_read ]"), 1)
    vector = f"""vector_loop:
  %vi = phi i64 [ 0, %entry ], [ %vector_next, %vector_advance ]
  %vector_remaining = sub i64 {limit}, %vi
  %vector_enough = icmp uge i64 %vector_remaining, {count}
  br i1 %vector_enough, label %vector_read, label %{loop}
vector_read:
  %vap = getelementptr i{bits}, ptr {a}, i64 %vi
  %vbp = getelementptr i{bits}, ptr {b}, i64 %vi
  %va = load <{width} x i{bits}>, ptr %vap, align {bits // 8}
  %vb = load <{width} x i{bits}>, ptr %vbp, align {bits // 8}
  %difference0 = xor <{width} x i{bits}> %va, %vb
"""
    if equality:
        vector += f"""  %second = add i64 %vi, {width}
  %va2p = getelementptr i{bits}, ptr {a}, i64 %second
  %vb2p = getelementptr i{bits}, ptr {b}, i64 %second
  %va2 = load <{width} x i{bits}>, ptr %va2p, align {bits // 8}
  %vb2 = load <{width} x i{bits}>, ptr %vb2p, align {bits // 8}
  %difference1 = xor <{width} x i{bits}> %va2, %vb2
  %difference = or <{width} x i{bits}> %difference0, %difference1
"""
    else:
        vector += f"  %difference = or <{width} x i{bits}> %difference0, zeroinitializer\n"
    vector += f"""  %vector_equal = icmp eq <{width} x i{bits}> %difference, zeroinitializer
  %vector_mask = bitcast <{width} x i1> %vector_equal to i{width}
  %vector_match = icmp eq i{width} %vector_mask, -1
  br i1 %vector_match, label %vector_advance, label %{ 'different' if equality else loop }
vector_advance:
  %vector_next = add i64 %vi, {count if equality else width}
  br label %vector_loop
"""
    return body.replace(loop + ":\n", vector + loop + ":\n", 1)


def compare_dispatch(body, bits):
    header = body[:body.index("entry:\n")]
    signature = re.fullmatch(
        r"define internal i32 @([\w.]+)\((%[\w.]+) %left, (%[\w.]+) %right\) nounwind \{\n",
        header,
    )
    if signature is None:
        raise ValueError(f"unexpected compare signature: {header.strip()}")
    name, left_type, right_type = signature.groups()
    threshold = (16 // (bits // 8)) * 2
    scalar = body.replace(f"@{name}(", f"@{name}.scalar(", 1)
    scalar = scalar.replace(" nounwind {", " nounwind alwaysinline {", 1)
    bulk = compare_vector(body, bits).replace(f"@{name}(", f"@{name}.bulk(", 1)
    bulk = bulk.replace(" nounwind {", " nounwind noinline {", 1)
    wrapper = header.replace(" nounwind {", " nounwind noinline {", 1) + f"""entry:
  %left_length = extractvalue {left_type} %left, 1
  %right_length = extractvalue {right_type} %right, 1
  %shorter = icmp ult i64 %left_length, %right_length
  %length = select i1 %shorter, i64 %left_length, i64 %right_length
  %bulk = icmp uge i64 %length, {threshold}
  br i1 %bulk, label %compare_bulk, label %compare_scalar
compare_scalar:
  %scalar = call i32 @{name}.scalar({left_type} %left, {right_type} %right)
  ret i32 %scalar
compare_bulk:
  %result = call i32 @{name}.bulk({left_type} %left, {right_type} %right)
  ret i32 %result
}}
"""
    return scalar + bulk + wrapper


def utf8_block():
    # Each block starts at a scalar boundary. Incomplete/unhandled blocks return
    # zero, so the scalar decoder validates the entire failed window.
    text = """define internal { i64, i64 } @tz.utf8.block(ptr %source, ptr %output) nounwind {
entry:
  %v = load <16 x i8>, ptr %source, align 1
  %ascii = icmp ult <16 x i8> %v, """ + constant(16, 8, 128) + """
  %am = bitcast <16 x i1> %ascii to i16
  %all = icmp eq i16 %am, -1
  br i1 %all, label %ascii_write, label %test2
ascii_write:
  %aw = zext <16 x i8> %v to <16 x i16>
  %astore = icmp ne ptr %output, null
  br i1 %astore, label %ascii_store, label %ascii_done
ascii_store:
  store <16 x i16> %aw, ptr %output, align 2
  br label %ascii_done
ascii_done:
  ret { i64, i64 } { i64 16, i64 16 }
"""
    for width, n in [(2, 8), (3, 4), (4, 4)]:
        text += f"test{width}:\n"
        for j in range(width):
            text += f"  %b{width}_{j} = " + shuffle(16, 8, [i * width + j for i in range(n)]) + "\n"
        lo, hi = {2: (194, 223), 3: (224, 239), 4: (240, 244)}[width]
        text += f"  %lo{width} = icmp uge <{n} x i8> %b{width}_0, {constant(n,8,lo)}\n"
        text += f"  %hi{width} = icmp ule <{n} x i8> %b{width}_0, {constant(n,8,hi)}\n"
        text += f"  %ok{width}_0 = and <{n} x i1> %lo{width}, %hi{width}\n"
        for j in range(1, width):
            text += f"  %tag{width}_{j} = and <{n} x i8> %b{width}_{j}, {constant(n,8,192)}\n"
            text += f"  %cont{width}_{j} = icmp eq <{n} x i8> %tag{width}_{j}, {constant(n,8,128)}\n"
            text += f"  %ok{width}_{j} = and <{n} x i1> %ok{width}_{j-1}, %cont{width}_{j}\n"
        for j in range(width):
            mask = (127 >> width) if j == 0 else 63
            text += f"  %payload{width}_{j} = and <{n} x i8> %b{width}_{j}, {constant(n,8,mask)}\n"
            text += f"  %wide{width}_{j} = zext <{n} x i8> %payload{width}_{j} to <{n} x i32>\n"
        text += f"  %scalar{width}_0 = shl <{n} x i32> %wide{width}_0, {constant(n,32,6*(width-1))}\n"
        for j in range(1, width):
            text += f"  %shift{width}_{j} = shl <{n} x i32> %wide{width}_{j}, {constant(n,32,6*(width-1-j))}\n"
            text += f"  %scalar{width}_{j} = or <{n} x i32> %scalar{width}_{j-1}, %shift{width}_{j}\n"
        scalar = f"%scalar{width}_{width-1}"
        minimum = {2: 128, 3: 2048, 4: 65536}[width]
        text += f"  %shortest{width} = icmp uge <{n} x i32> {scalar}, {constant(n,32,minimum)}\n"
        text += f"  %range{width} = icmp ule <{n} x i32> {scalar}, {constant(n,32,0x10ffff)}\n"
        text += f"  %offset{width} = sub <{n} x i32> {scalar}, {constant(n,32,0xd800)}\n"
        text += f"  %nosurrogate{width} = icmp uge <{n} x i32> %offset{width}, {constant(n,32,2048)}\n"
        text += f"  %legal{width}a = and <{n} x i1> %shortest{width}, %range{width}\n"
        text += f"  %legal{width}b = and <{n} x i1> %legal{width}a, %nosurrogate{width}\n"
        text += f"  %legal{width} = and <{n} x i1> %legal{width}b, %ok{width}_{width-1}\n"
        text += f"  %mask{width} = bitcast <{n} x i1> %legal{width} to i{n}\n"
        text += f"  %all{width} = icmp eq i{n} %mask{width}, -1\n"
        following = f"test{width+1}" if width < 4 else "fallback"
        text += f"  br i1 %all{width}, label %write{width}, label %{following}\nwrite{width}:\n"
        text += f"  %do_store{width} = icmp ne ptr %output, null\n  br i1 %do_store{width}, label %store{width}, label %done{width}\nstore{width}:\n"
        if width == 4:
            text += f"  %remainder4 = sub <4 x i32> {scalar}, {constant(4,32,65536)}\n"
            text += f"  %high4a = lshr <4 x i32> %remainder4, {constant(4,32,10)}\n"
            text += f"  %high4b = add <4 x i32> %high4a, {constant(4,32,0xd800)}\n"
            text += f"  %low4a = and <4 x i32> %remainder4, {constant(4,32,1023)}\n"
            text += f"  %low4b = add <4 x i32> %low4a, {constant(4,32,0xdc00)}\n"
            text += "  %high4 = trunc <4 x i32> %high4b to <4 x i16>\n  %low4 = trunc <4 x i32> %low4b to <4 x i16>\n"
            text += "  %units4 = shufflevector <4 x i16> %high4, <4 x i16> %low4, <8 x i32> <i32 0, i32 4, i32 1, i32 5, i32 2, i32 6, i32 3, i32 7>\n"
            text += "  store <8 x i16> %units4, ptr %output, align 2\n"
        else:
            text += f"  %units{width} = trunc <{n} x i32> {scalar} to <{n} x i16>\n"
            text += f"  store <{n} x i16> %units{width}, ptr %output, align 2\n"
        text += f"  br label %done{width}\ndone{width}:\n  ret {{ i64, i64 }} {{ i64 {n*width}, i64 {n*(2 if width==4 else 1)} }}\n"
    return text + utf8_mixed()


def utf8_mixed():
    text = """fallback:
  %count_only = icmp eq ptr %output, null
  br i1 %count_only, label %mixed, label %no_block
mixed:
"""
    for distance, minimum in [(1,194),(2,224),(3,240)]:
        indexes = [0] * distance + list(range(16,32-distance))
        mask = "<" + ", ".join(f"i32 {i}" for i in indexes) + ">"
        text += f"  %previous{distance} = shufflevector <16 x i8> zeroinitializer, <16 x i8> %v, <16 x i32> {mask}\n"
        text += f"  %lead_offset{distance} = sub <16 x i8> %previous{distance}, {constant(16,8,minimum)}\n"
        text += f"  %lead{distance} = icmp ult <16 x i8> %lead_offset{distance}, {constant(16,8,245-minimum)}\n"
    text += """  %expected12 = or <16 x i1> %lead1, %lead2
  %expected = or <16 x i1> %expected12, %lead3
  %tags = and <16 x i8> %v, """ + constant(16,8,192) + """
  %continuation = icmp eq <16 x i8> %tags, """ + constant(16,8,128) + """
  %bad_structure = xor <16 x i1> %expected, %continuation
  %legal_offset = sub <16 x i8> %v, """ + constant(16,8,194) + """
  %legal_lead = icmp ult <16 x i8> %legal_offset, """ + constant(16,8,51) + """
  %small_byte = icmp ult <16 x i8> %v, """ + constant(16,8,192) + """
  %legal_byte = or <16 x i1> %legal_lead, %small_byte
  %illegal_byte = xor <16 x i1> %legal_byte, """ + constant(16,1,1) + """
  %bad0 = or <16 x i1> %bad_structure, %illegal_byte
"""
    for j, (lead, limit, comparison) in enumerate([(224,160,"ult"),(237,160,"uge"),(240,144,"ult"),(244,144,"uge")],1):
        text += f"  %special{j} = icmp eq <16 x i8> %previous1, {constant(16,8,lead)}\n"
        text += f"  %outside{j} = icmp {comparison} <16 x i8> %v, {constant(16,8,limit)}\n"
        text += f"  %special_bad{j} = and <16 x i1> %special{j}, %outside{j}\n"
        text += f"  %bad{j} = or <16 x i1> %bad{j-1}, %special_bad{j}\n"
    text += """  %bad_mask = bitcast <16 x i1> %bad4 to i16
  %mixed_legal = icmp eq i16 %bad_mask, 0
"""
    for position, maximum in [(15,194),(14,224),(13,240)]:
        text += f"  %tail{position} = extractelement <16 x i8> %v, i32 {position}\n"
        text += f"  %complete{position} = icmp ult i8 %tail{position}, {maximum}\n"
    text += """  %complete_pair = and i1 %complete15, %complete14
  %complete = and i1 %complete_pair, %complete13
  %mixed_valid = and i1 %mixed_legal, %complete
  br i1 %mixed_valid, label %mixed_count, label %no_block
mixed_count:
  %starts = xor <16 x i1> %continuation, """ + constant(16,1,1) + """
  %four_leads = icmp uge <16 x i8> %v, """ + constant(16,8,240) + """
  %start_mask = bitcast <16 x i1> %starts to i16
  %four_mask = bitcast <16 x i1> %four_leads to i16
"""
    for name in ["start", "four"]:
        text += f"""  %{name}_half = lshr i16 %{name}_mask, 1
  %{name}_odd = and i16 %{name}_half, 21845
  %{name}_a = sub i16 %{name}_mask, %{name}_odd
  %{name}_shift2 = lshr i16 %{name}_a, 2
  %{name}_even2 = and i16 %{name}_a, 13107
  %{name}_odd2 = and i16 %{name}_shift2, 13107
  %{name}_b = add i16 %{name}_even2, %{name}_odd2
  %{name}_shift4 = lshr i16 %{name}_b, 4
  %{name}_c0 = add i16 %{name}_b, %{name}_shift4
  %{name}_c = and i16 %{name}_c0, 3855
  %{name}_shift8 = lshr i16 %{name}_c, 8
  %{name}_d = add i16 %{name}_c, %{name}_shift8
  %{name}_count = and i16 %{name}_d, 31
"""
    text += """  %unit_count = add i16 %start_count, %four_count
  %units = zext i16 %unit_count to i64
  %count0 = insertvalue { i64, i64 } zeroinitializer, i64 16, 0
  %count1 = insertvalue { i64, i64 } %count0, i64 %units, 1
  ret { i64, i64 } %count1
no_block:
  ret { i64, i64 } zeroinitializer
}
"""
    return text


def utf16_block():
    text = """define internal { i64, i64 } @tz.utf16.block(ptr %source, ptr %output) nounwind {
entry:
  %v = load <8 x i16>, ptr %source, align 2
  %count_only = icmp eq ptr %output, null
  br i1 %count_only, label %count_ascii, label %test1
count_ascii:
  %count_ascii_lanes = icmp ult <8 x i16> %v, """ + constant(8, 16, 128) + """
  %count_ascii_mask = bitcast <8 x i1> %count_ascii_lanes to i8
  %count_all_ascii = icmp eq i8 %count_ascii_mask, -1
  br i1 %count_all_ascii, label %count_ascii_done, label %count_bmp
count_ascii_done:
  ret { i64, i64 } { i64 8, i64 8 }
count_bmp:
  %count_masked = and <8 x i16> %v, """ + constant(8, 16, 0xf800) + """
  %count_surrogates = icmp eq <8 x i16> %count_masked, """ + constant(8, 16, 0xd800) + """
  %count_mask = bitcast <8 x i1> %count_surrogates to i8
  %count_none = icmp eq i8 %count_mask, 0
  br i1 %count_none, label %count_widths, label %fallback
count_widths:
  %count_two = icmp uge <8 x i16> %v, """ + constant(8, 16, 128) + """
  %count_three = icmp uge <8 x i16> %v, """ + constant(8, 16, 2048) + """
  %count_twos = zext <8 x i1> %count_two to <8 x i16>
  %count_threes = zext <8 x i1> %count_three to <8 x i16>
  %count_extra = add <8 x i16> %count_twos, %count_threes
  %count_sum = call i16 @llvm.vector.reduce.add.v8i16(<8 x i16> %count_extra)
  %count_wide = zext i16 %count_sum to i64
  %count_bytes = add i64 %count_wide, 8
  %count_a = insertvalue { i64, i64 } zeroinitializer, i64 8, 0
  %count_b = insertvalue { i64, i64 } %count_a, i64 %count_bytes, 1
  ret { i64, i64 } %count_b
test1:
"""
    for width, lo, hi in [(1, 0, 127), (2, 128, 2047), (3, 2048, 65535)]:
        if width > 1:
            text += f"test{width}:\n"
        text += f"  %lo{width} = icmp uge <8 x i16> %v, {constant(8,16,lo)}\n"
        text += f"  %hi{width} = icmp ule <8 x i16> %v, {constant(8,16,hi)}\n"
        text += f"  %range{width} = and <8 x i1> %lo{width}, %hi{width}\n"
        if width == 3:
            text += f"  %offset = sub <8 x i16> %v, {constant(8,16,0xd800)}\n  %nonsurrogate = icmp uge <8 x i16> %offset, {constant(8,16,2048)}\n  %legal3 = and <8 x i1> %range3, %nonsurrogate\n"
        mask = "%legal3" if width == 3 else f"%range{width}"
        text += f"  %mask{width} = bitcast <8 x i1> {mask} to i8\n  %all{width} = icmp eq i8 %mask{width}, -1\n"
        following = f"test{width+1}" if width < 3 else "fallback"
        text += f"  br i1 %all{width}, label %write{width}, label %{following}\nwrite{width}:\n  %do_store{width} = icmp ne ptr %output, null\n  br i1 %do_store{width}, label %store{width}, label %done{width}\nstore{width}:\n"
        for j in range(width):
            shift = 6 * (width - j - 1)
            mask = 127 if width == 1 else ((31 if width == 2 else 15) if j == 0 else 63)
            tag = 0 if width == 1 else ((192 if width == 2 else 224) if j == 0 else 128)
            text += f"  %shift{width}_{j} = lshr <8 x i16> %v, {constant(8,16,shift)}\n"
            text += f"  %payload{width}_{j} = and <8 x i16> %shift{width}_{j}, {constant(8,16,mask)}\n"
            text += f"  %tag{width}_{j} = or <8 x i16> %payload{width}_{j}, {constant(8,16,tag)}\n"
            text += f"  %byte{width}_{j} = trunc <8 x i16> %tag{width}_{j} to <8 x i8>\n"
        if width == 1:
            text += "  store <8 x i8> %byte1_0, ptr %output, align 1\n"
        elif width == 2:
            indexes = ", ".join(f"i32 {i}" for j in range(8) for i in (j, j+8))
            text += f"  %bytes2 = shufflevector <8 x i8> %byte2_0, <8 x i8> %byte2_1, <16 x i32> <{indexes}>\n  store <16 x i8> %bytes2, ptr %output, align 1\n"
        else:
            # Three 8-byte stores keep the exact 24-byte allocation boundary.
            text += "  %joined = shufflevector <8 x i8> %byte3_0, <8 x i8> %byte3_1, <16 x i32> <" + ", ".join(f"i32 {i}" for i in range(16)) + ">\n"
            text += "  %third = shufflevector <8 x i8> %byte3_2, <8 x i8> zeroinitializer, <16 x i32> <" + ", ".join(f"i32 {i}" for i in range(16)) + ">\n"
            for group in range(3):
                indexes = [((i // 3) + (0, 8, 16)[i % 3]) for i in range(group*8, group*8+8)]
                text += f"  %part{group} = shufflevector <16 x i8> %joined, <16 x i8> %third, <8 x i32> <" + ", ".join(f"i32 {i}" for i in indexes) + ">\n"
                text += f"  %out{group} = getelementptr i8, ptr %output, i64 {group*8}\n  store <8 x i8> %part{group}, ptr %out{group}, align 1\n"
        text += f"  br label %done{width}\ndone{width}:\n  ret {{ i64, i64 }} {{ i64 8, i64 {8*width} }}\n"
    return text + "fallback:\n  ret { i64, i64 } zeroinitializer\n}\n"


def vector_loop(body, utf8, counting):
    # ponytail: later bulk runs stay scalar; re-probe only if end-to-end measurements justify it.
    width = 16 if utf8 else 8
    srcbits, dstbits = (8, 16) if utf8 else (16, 8)
    loop, entry = ("count", "entry") if counting else ("write", "unicode")
    limit = "byte_length" if utf8 else "unit_length"
    body = body.replace(f"label %{loop}", "label %block_loop", 1)
    body = body.replace(f"[ 0, %{entry} ]", "[ %block_i, %block_scalar_start ]", 1)
    body = body.replace(f"[ 0, %{entry} ]", "[ %block_total, %block_scalar_start ]", 1)
    probe = f"""block_loop:
  %block_i = phi i64 [ 0, %{entry} ], [ %block_next, %block_advance ]
  %block_total = phi i64 [ 0, %{entry} ], [ %block_total_next, %block_advance ]
  %block_done = icmp eq i64 %block_i, %{limit}
  br i1 %block_done, label %block_scalar_start, label %block_width
block_width:
  %block_remaining = sub i64 %{limit}, %block_i
  %block_enough = icmp uge i64 %block_remaining, {width}
  br i1 %block_enough, label %block_read, label %block_scalar_start
block_read:
  %block_source = getelementptr i{srcbits}, ptr %source, i64 %block_i
"""
    out = "null"
    if not counting:
        probe += f"  %block_destination = getelementptr inbounds i{dstbits}, ptr %data, i64 %block_total\n"
        out = "%block_destination"
    probe += f"""  %block = call {{ i64, i64 }} @tz.utf{8 if utf8 else 16}.block(ptr %block_source, ptr {out})
  %block_consumed = extractvalue {{ i64, i64 }} %block, 0
  %block_produced = extractvalue {{ i64, i64 }} %block, 1
  %block_success = icmp ne i64 %block_consumed, 0
  br i1 %block_success, label %block_advance, label %block_scalar_start
block_advance:
  %block_next = add i64 %block_i, %block_consumed
  %block_total_next = add i64 %block_total, %block_produced
"""
    if counting and utf8:
        probe += "  %block_fits = icmp ule i64 %block_total_next, 9007199254740991\n  br i1 %block_fits, label %block_loop, label %fail\n"
    else:
        probe += "  br label %block_loop\n"
    probe += f"block_scalar_start:\n  br label %{loop}\n"
    return body.replace(f"{loop}:\n", probe + f"{loop}:\n", 1)


def validation_vector(body):
    body = body.replace("label %loop", "label %bulk", 1)
    body = body.replace("[ 0, %entry ]", "[ %bulk_i, %scalar_start ]", 1)
    return body.replace("loop:\n", """bulk:
  %bulk_i = phi i64 [ 0, %entry ], [ %bulk_next, %bulk_advance ]
  %remaining = sub i64 %unit_length, %bulk_i
  %enough = icmp uge i64 %remaining, 8
  br i1 %enough, label %bulk_read, label %scalar_start
bulk_read:
  %vp = getelementptr i16, ptr %source, i64 %bulk_i
  %v = load <8 x i16>, ptr %vp, align 2
  %masked = and <8 x i16> %v, """ + constant(8,16,0xf800) + """
  %surrogates = icmp eq <8 x i16> %masked, """ + constant(8,16,0xd800) + """
  %mask = bitcast <8 x i1> %surrogates to i8
  %none = icmp eq i8 %mask, 0
  br i1 %none, label %bulk_advance, label %scalar_start
bulk_advance:
  %bulk_next = add i64 %bulk_i, 8
  br label %bulk
scalar_start:
  br label %loop
loop:
""", 1)


def validation_utf8(body):
    body = body.replace("label %loop\n", "label %block_loop\n", 1)
    body = body.replace("[ 0, %entry ]", "[ %block_i, %block_scalar_start ]", 1)
    return body.replace("loop:\n", """block_loop:
  %block_i = phi i64 [ 0, %entry ], [ %block_next, %block_advance ]
  %remaining = sub i64 %byte_length, %block_i
  %enough = icmp uge i64 %remaining, 16
  br i1 %enough, label %block_read, label %block_scalar_start
block_read:
  %p = getelementptr i8, ptr %source, i64 %block_i
  %result = call { i64, i64 } @tz.utf8.block(ptr %p, ptr null)
  %consumed = extractvalue { i64, i64 } %result, 0
  %success = icmp ne i64 %consumed, 0
  br i1 %success, label %block_advance, label %block_scalar_start
block_advance:
  %block_next = add i64 %block_i, %consumed
  br label %block_loop
block_scalar_start:
  br label %loop
loop:
""", 1)


def main():
    scalar_path = ROOT / "string_scalar.ll"
    scalar = scalar_path.read_text()
    functions = {m[1]: m[0] for m in PATTERN.finditer(scalar)}
    vector = []
    for name, body in functions.items():
        if name.endswith(".from_ascii"):
            body = ascii_copy(name == "tz.string.from_ascii")
        elif name.endswith(".equal"):
            body = compare_vector(body, 8 if "utf8string" in name else 16)
        elif name.endswith(".compare"):
            body = compare_dispatch(body, 8 if "utf8string" in name else 16)
        elif name.endswith("_length"):
            body = vector_loop(body, name == "tz.string.utf8_to_utf16_length", True)
        elif name.endswith(".from_utf8") or name.endswith(".from_string"):
            utf8 = name.endswith(".from_utf8")
            length_name = "tz.string.utf8_to_utf16_length" if utf8 else "tz.utf8string.utf16_to_utf8_length"
            limit = "byte_length" if utf8 else "unit_length"
            body = body[:body.index("entry:")] + f"entry:\n  %length = call i64 @{length_name}(ptr %source, i64 %{limit})\n  %valid = icmp sge i64 %length, 0\n  br i1 %valid, label %allocate, label %fail\n" + body[body.index("allocate:"):]
            if utf8:
                body = vector_loop(body, True, False)
            else:
                write = """define internal void @tz.utf8string.write_utf16(ptr %source, i64 %unit_length, ptr nonnull %data) nounwind alwaysinline {
unicode:
  br label %write
""" + body[body.index("write:\n"):body.index("exit:\n")] + "exit:\n  ret void\n}\n"
                vector.append(vector_loop(write, False, False))
                body = body[:body.index("write:\n")].replace(
                    "  br label %write\n",
                    "  call void @tz.utf8string.write_utf16(ptr %source, i64 %unit_length, ptr %data)\n"
                    "  ret %tz.utf8string %result\n",
                ) + body[body.index("fail:\n"):]
            if utf8:
                # Re-enter the original count path only on failure to preserve
                # its encoding/size trap classification and evaluation order.
                body = body.replace(
                    "fail:\n  call void @llvm.trap()\n  unreachable\n",
                    "fail:\n  %legacy = call %tz.string @tz.string.from_utf8.scalar_failure(ptr %source, i64 %byte_length)\n  ret %tz.string %legacy\n",
                )
            header, implementation = body.split("entry:\n", 1)
            if utf8:
                body = header + """entry:
  %short = icmp ult i64 %byte_length, 16
  br i1 %short, label %scalar_short, label %vector_entry
scalar_short:
  %short_result = call %tz.string @tz.string.from_utf8.short(ptr %source, i64 %byte_length)
  ret %tz.string %short_result
vector_entry:
""" + implementation
            else:
                vector.append((header + "entry:\n" + implementation)
                              .replace("@tz.utf8string.from_string(", "@tz.utf8string.from_string.bulk(", 1)
                              .replace(" nounwind {", " nounwind noinline {", 1))
                body = header.replace(" nounwind {", " nounwind alwaysinline {") + """entry:
  %prefix_long = icmp uge i64 %unit_length, 16
  br i1 %prefix_long, label %prefix_fits_check, label %scalar
scalar:
  %scalar_result = call %tz.utf8string @tz.utf8string.from_string.short(ptr %source, i64 %unit_length)
  ret %tz.utf8string %scalar_result
prefix_fits_check:
  %prefix_fits = icmp ule i64 %unit_length, 9007199254740991
  br i1 %prefix_fits, label %prefix_read, label %fail
prefix_read:  %prefix = load <8 x i16>, ptr %source, align 2
  %prefix_masked = and <8 x i16> %prefix, """ + constant(8, 16, 0xf800) + """
  %prefix_surrogates = icmp eq <8 x i16> %prefix_masked, """ + constant(8, 16, 0xd800) + """
  %prefix_mask = bitcast <8 x i1> %prefix_surrogates to i8
  %prefix_none = icmp eq i8 %prefix_mask, 0
  br i1 %prefix_none, label %bulk, label %scalar
bulk:
  %bulk_result = call %tz.utf8string @tz.utf8string.from_string.bulk(ptr %source, i64 %unit_length)
  ret %tz.utf8string %bulk_result
fail:
  call void @llvm.trap()
  unreachable
}
"""
        elif name.endswith(".is_well_formed") or name.endswith(".to_well_formed"):
            body = validation_vector(body)
        elif name.endswith(".validate_utf8"):
            body = validation_utf8(body)
        vector.append(body)
    for name in ["tz.string.from_ascii", "tz.utf8string.from_ascii",
                 "tz.string.from_utf8", "tz.utf8string.from_string"]:
        if name != "tz.utf8string.from_string":
            suffix = ".scalar_failure" if name == "tz.string.from_utf8" else ".scalar"
            body = functions[name].replace(f"@{name}(", f"@{name}{suffix}(", 1)
            for ascii_name in ["tz.string.from_ascii", "tz.utf8string.from_ascii"]:
                body = body.replace(f"@{ascii_name}(", f"@{ascii_name}.scalar(")
            vector.append(body.replace(" nounwind {", " nounwind alwaysinline {"))
        if name.endswith(".from_utf8") or name.endswith(".from_string"):
            short = functions[name].replace(f"@{name}(", f"@{name}.short(", 1)
            for ascii_name in ["tz.string.from_ascii", "tz.utf8string.from_ascii"]:
                short = short.replace(f"@{ascii_name}(", f"@{ascii_name}.short(")
            vector.append(short.replace(" nounwind {", " nounwind alwaysinline {"))
    for utf8 in [True, False]:
        vector.append(ascii_copy(utf8, 8, ".short").replace(" nounwind {", " nounwind alwaysinline {"))
    (ROOT / "string_v128.ll").write_text("; Generated by generate_utf.py; retain string_scalar.ll as reference.\n\n" +
                                       "\n".join(vector) + "\n" + utf8_block() + "\n" +
                                       utf16_block().replace(" nounwind {", " nounwind alwaysinline {") +
                                       "\ndeclare i16 @llvm.vector.reduce.add.v8i16(<8 x i16>)\n")


if __name__ == "__main__":
    main()
