; Generated from numeric.c by generate.py. Do not edit.

%struct.tzrt_format = type { i32, i32, i32, i32, i32, i32, i32, i32, i32 }
%struct.tzrt_number = type { %struct.tzrt_big, i32, i32, i32 }
%struct.tzrt_big = type { i32, [1400 x i32] }

@.str = private unnamed_addr constant [4 x i8] c"nan\00", align 1
@.str.1 = private unnamed_addr constant [4 x i8] c"inf\00", align 1

; Function Attrs: nounwind memory(argmem: readwrite, inaccessiblemem: readwrite)
define weak hidden void @tz_soft_op(ptr noundef writeonly %0, ptr noundef readonly %1, ptr noundef readonly %2, i32 noundef %3, i32 noundef %4) local_unnamed_addr #0 {
  %6 = alloca %struct.tzrt_format, align 8
  %7 = alloca %struct.tzrt_number, align 4
  %8 = alloca %struct.tzrt_number, align 4
  %9 = alloca %struct.tzrt_big, align 4
  %10 = alloca %struct.tzrt_big, align 4
  %11 = alloca %struct.tzrt_big, align 4
  call void @llvm.lifetime.start.p0(i64 36, ptr nonnull %6) #10
  switch i32 %3, label %75 [
    i32 0, label %12
    i32 1, label %21
    i32 2, label %30
    i32 3, label %39
    i32 4, label %48
    i32 5, label %57
    i32 6, label %66
  ]

12:                                               ; preds = %5
  store i32 2, ptr %6, align 8, !tbaa !4, !alias.scope !9
  %13 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 11, ptr %13, align 4, !tbaa !12, !alias.scope !9
  %14 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 -24, ptr %14, align 8, !tbaa !13, !alias.scope !9
  %15 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 5, ptr %15, align 4, !tbaa !14, !alias.scope !9
  %16 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 10, ptr %16, align 8, !tbaa !15, !alias.scope !9
  %17 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 15, ptr %17, align 4, !tbaa !16, !alias.scope !9
  %18 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 16, ptr %18, align 8, !tbaa !17, !alias.scope !9
  %19 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 0, ptr %19, align 4, !tbaa !18, !alias.scope !9
  %20 = getelementptr inbounds i8, ptr %6, i64 32
  store i32 1, ptr %20, align 8, !tbaa !19, !alias.scope !9
  br label %88

21:                                               ; preds = %5
  store i32 2, ptr %6, align 8, !tbaa !4, !alias.scope !9
  %22 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 24, ptr %22, align 4, !tbaa !12, !alias.scope !9
  %23 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 -149, ptr %23, align 8, !tbaa !13, !alias.scope !9
  %24 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 104, ptr %24, align 4, !tbaa !14, !alias.scope !9
  %25 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 23, ptr %25, align 8, !tbaa !15, !alias.scope !9
  %26 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 127, ptr %26, align 4, !tbaa !16, !alias.scope !9
  %27 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 32, ptr %27, align 8, !tbaa !17, !alias.scope !9
  %28 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 0, ptr %28, align 4, !tbaa !18, !alias.scope !9
  %29 = getelementptr inbounds i8, ptr %6, i64 32
  store i32 1, ptr %29, align 8, !tbaa !19, !alias.scope !9
  br label %88

30:                                               ; preds = %5
  store i32 2, ptr %6, align 8, !tbaa !4, !alias.scope !9
  %31 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 53, ptr %31, align 4, !tbaa !12, !alias.scope !9
  %32 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 -1074, ptr %32, align 8, !tbaa !13, !alias.scope !9
  %33 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 971, ptr %33, align 4, !tbaa !14, !alias.scope !9
  %34 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 52, ptr %34, align 8, !tbaa !15, !alias.scope !9
  %35 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 1023, ptr %35, align 4, !tbaa !16, !alias.scope !9
  %36 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 64, ptr %36, align 8, !tbaa !17, !alias.scope !9
  %37 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 0, ptr %37, align 4, !tbaa !18, !alias.scope !9
  %38 = getelementptr inbounds i8, ptr %6, i64 32
  store i32 1, ptr %38, align 8, !tbaa !19, !alias.scope !9
  br label %88

39:                                               ; preds = %5
  store i32 2, ptr %6, align 8, !tbaa !4, !alias.scope !9
  %40 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 113, ptr %40, align 4, !tbaa !12, !alias.scope !9
  %41 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 -16494, ptr %41, align 8, !tbaa !13, !alias.scope !9
  %42 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 16271, ptr %42, align 4, !tbaa !14, !alias.scope !9
  %43 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 112, ptr %43, align 8, !tbaa !15, !alias.scope !9
  %44 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 16383, ptr %44, align 4, !tbaa !16, !alias.scope !9
  %45 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 128, ptr %45, align 8, !tbaa !17, !alias.scope !9
  %46 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 0, ptr %46, align 4, !tbaa !18, !alias.scope !9
  %47 = getelementptr inbounds i8, ptr %6, i64 32
  store i32 1, ptr %47, align 8, !tbaa !19, !alias.scope !9
  br label %88

48:                                               ; preds = %5
  store i32 10, ptr %6, align 8, !tbaa !4, !alias.scope !9
  %49 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 7, ptr %49, align 4, !tbaa !12, !alias.scope !9
  %50 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 -101, ptr %50, align 8, !tbaa !13, !alias.scope !9
  %51 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 90, ptr %51, align 4, !tbaa !14, !alias.scope !9
  %52 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 23, ptr %52, align 8, !tbaa !15, !alias.scope !9
  %53 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 101, ptr %53, align 4, !tbaa !16, !alias.scope !9
  %54 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 32, ptr %54, align 8, !tbaa !17, !alias.scope !9
  %55 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 0, ptr %55, align 4, !tbaa !18, !alias.scope !9
  %56 = getelementptr inbounds i8, ptr %6, i64 32
  store i32 1, ptr %56, align 8, !tbaa !19, !alias.scope !9
  br label %88

57:                                               ; preds = %5
  store i32 10, ptr %6, align 8, !tbaa !4, !alias.scope !9
  %58 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 16, ptr %58, align 4, !tbaa !12, !alias.scope !9
  %59 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 -398, ptr %59, align 8, !tbaa !13, !alias.scope !9
  %60 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 369, ptr %60, align 4, !tbaa !14, !alias.scope !9
  %61 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 53, ptr %61, align 8, !tbaa !15, !alias.scope !9
  %62 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 398, ptr %62, align 4, !tbaa !16, !alias.scope !9
  %63 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 64, ptr %63, align 8, !tbaa !17, !alias.scope !9
  %64 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 0, ptr %64, align 4, !tbaa !18, !alias.scope !9
  %65 = getelementptr inbounds i8, ptr %6, i64 32
  store i32 1, ptr %65, align 8, !tbaa !19, !alias.scope !9
  br label %88

66:                                               ; preds = %5
  store i32 10, ptr %6, align 8, !tbaa !4, !alias.scope !9
  %67 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 34, ptr %67, align 4, !tbaa !12, !alias.scope !9
  %68 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 -6176, ptr %68, align 8, !tbaa !13, !alias.scope !9
  %69 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 6111, ptr %69, align 4, !tbaa !14, !alias.scope !9
  %70 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 113, ptr %70, align 8, !tbaa !15, !alias.scope !9
  %71 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 6176, ptr %71, align 4, !tbaa !16, !alias.scope !9
  %72 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 128, ptr %72, align 8, !tbaa !17, !alias.scope !9
  %73 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 0, ptr %73, align 4, !tbaa !18, !alias.scope !9
  %74 = getelementptr inbounds i8, ptr %6, i64 32
  store i32 1, ptr %74, align 8, !tbaa !19, !alias.scope !9
  br label %88

75:                                               ; preds = %5
  %76 = and i32 %3, 7
  %77 = shl nuw nsw i32 8, %76
  store i32 2, ptr %6, align 8, !tbaa !4, !alias.scope !9
  %78 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 %77, ptr %78, align 4, !tbaa !12, !alias.scope !9
  %79 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 0, ptr %79, align 8, !tbaa !13, !alias.scope !9
  %80 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 0, ptr %80, align 4, !tbaa !14, !alias.scope !9
  %81 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 0, ptr %81, align 8, !tbaa !15, !alias.scope !9
  %82 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 0, ptr %82, align 4, !tbaa !16, !alias.scope !9
  %83 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 %77, ptr %83, align 8, !tbaa !17, !alias.scope !9
  %84 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 1, ptr %84, align 4, !tbaa !18, !alias.scope !9
  %85 = getelementptr inbounds i8, ptr %6, i64 32
  %86 = icmp slt i32 %3, 24
  %87 = zext i1 %86 to i32
  store i32 %87, ptr %85, align 8, !tbaa !19, !alias.scope !9
  br label %88

88:                                               ; preds = %12, %21, %30, %39, %48, %57, %66, %75
  %89 = phi i32 [ 16, %12 ], [ 32, %21 ], [ 64, %30 ], [ 128, %39 ], [ 32, %48 ], [ 64, %57 ], [ 128, %66 ], [ %77, %75 ]
  %90 = phi i32 [ 31, %12 ], [ 255, %21 ], [ 2047, %30 ], [ 32767, %39 ], [ 203, %48 ], [ 797, %57 ], [ 12353, %66 ], [ 1, %75 ]
  %91 = phi i32 [ 10, %12 ], [ 23, %21 ], [ 52, %30 ], [ 112, %39 ], [ 23, %48 ], [ 53, %57 ], [ 113, %66 ], [ 0, %75 ]
  %92 = phi i1 [ true, %12 ], [ true, %21 ], [ true, %30 ], [ true, %39 ], [ false, %48 ], [ false, %57 ], [ false, %66 ], [ true, %75 ]
  %93 = phi i32 [ 2, %12 ], [ 2, %21 ], [ 2, %30 ], [ 2, %39 ], [ 10, %48 ], [ 10, %57 ], [ 10, %66 ], [ 2, %75 ]
  call void @llvm.lifetime.start.p0(i64 5616, ptr nonnull %7) #10
  call fastcc void @decode(ptr sret(%struct.tzrt_number) align 4 %7, ptr noundef %1, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %6) #11
  call void @llvm.lifetime.start.p0(i64 5616, ptr nonnull %8) #10
  call fastcc void @decode(ptr sret(%struct.tzrt_number) align 4 %8, ptr noundef %2, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %6) #11
  %94 = icmp eq i32 %4, 1
  br i1 %94, label %95, label %99

95:                                               ; preds = %88
  %96 = getelementptr inbounds i8, ptr %8, i64 5608
  %97 = load i32, ptr %96, align 4, !tbaa !20
  %98 = xor i32 %97, 1
  store i32 %98, ptr %96, align 4, !tbaa !20
  br label %99

99:                                               ; preds = %95, %88
  %100 = icmp slt i32 %4, 2
  %101 = getelementptr inbounds i8, ptr %7, i64 5608
  %102 = load i32, ptr %101, align 4
  %103 = getelementptr inbounds i8, ptr %8, i64 5608
  %104 = load i32, ptr %103, align 4
  %105 = select i1 %100, i32 0, i32 %104
  %106 = xor i32 %105, %102
  %107 = getelementptr inbounds i8, ptr %7, i64 5612
  %108 = load i32, ptr %107, align 4, !tbaa !23
  %109 = icmp eq i32 %108, 2
  %110 = getelementptr inbounds i8, ptr %8, i64 5612
  %111 = load i32, ptr %110, align 4
  %112 = icmp eq i32 %111, 2
  %113 = select i1 %109, i1 true, i1 %112
  br i1 %113, label %158, label %114

114:                                              ; preds = %99
  br i1 %100, label %115, label %126

115:                                              ; preds = %114
  %116 = icmp ne i32 %108, 0
  %117 = icmp ne i32 %111, 0
  %118 = select i1 %116, i1 true, i1 %117
  br i1 %118, label %119, label %126

119:                                              ; preds = %115
  %120 = select i1 %116, i1 %117, i1 false
  %121 = icmp eq i32 %102, %104
  %122 = select i1 %121, i32 1, i32 2
  %123 = select i1 %120, i32 %122, i32 1
  %124 = icmp eq i32 %108, 0
  %125 = select i1 %124, i32 %104, i32 %102
  br label %158

126:                                              ; preds = %115, %114
  switch i32 %4, label %158 [
    i32 2, label %127
    i32 3, label %142
  ]

127:                                              ; preds = %126
  %128 = icmp ne i32 %108, 0
  %129 = icmp ne i32 %111, 0
  %130 = select i1 %128, i1 true, i1 %129
  br i1 %130, label %131, label %140

131:                                              ; preds = %127
  %132 = load i32, ptr %7, align 4
  %133 = icmp ne i32 %132, 0
  %134 = select i1 %128, i1 true, i1 %133
  %135 = load i32, ptr %8, align 4
  %136 = icmp ne i32 %135, 0
  %137 = select i1 %129, i1 true, i1 %136
  %138 = select i1 %134, i1 %137, i1 false
  %139 = select i1 %138, i32 1, i32 2
  br label %158

140:                                              ; preds = %127
  %141 = icmp eq i32 %4, 3
  br i1 %141, label %142, label %158

142:                                              ; preds = %126, %140
  %143 = icmp ne i32 %108, 0
  %144 = icmp ne i32 %111, 0
  %145 = select i1 %143, i1 %144, i1 false
  %146 = select i1 %145, i32 2, i32 1
  br i1 %143, label %158, label %147

147:                                              ; preds = %142
  br i1 %144, label %148, label %151

148:                                              ; preds = %147
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %7, i8 0, i64 5604, i1 false)
  %149 = getelementptr inbounds i8, ptr %8, i64 8
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5596) %149, i8 0, i64 5596, i1 false)
  store i32 1, ptr %8, align 4, !tbaa !24
  %150 = getelementptr inbounds i8, ptr %8, i64 4
  store i32 1, ptr %150, align 4
  br label %158

151:                                              ; preds = %147
  %152 = load i32, ptr %8, align 4, !tbaa !25
  %153 = icmp eq i32 %152, 0
  br i1 %153, label %154, label %158

154:                                              ; preds = %151
  %155 = load i32, ptr %7, align 4, !tbaa !25
  %156 = icmp eq i32 %155, 0
  %157 = select i1 %156, i32 2, i32 1
  br label %158

158:                                              ; preds = %142, %126, %99, %119, %140, %151, %154, %148, %131
  %159 = phi i32 [ %123, %119 ], [ %139, %131 ], [ 0, %148 ], [ 0, %151 ], [ %157, %154 ], [ 0, %140 ], [ 2, %99 ], [ 0, %126 ], [ %146, %142 ]
  %160 = phi i32 [ %125, %119 ], [ %106, %131 ], [ %106, %148 ], [ %106, %151 ], [ %106, %154 ], [ %106, %140 ], [ %106, %99 ], [ %106, %126 ], [ %106, %142 ]
  %161 = icmp eq i32 %159, 0
  br i1 %161, label %212, label %162

162:                                              ; preds = %158
  %163 = sext i32 %160 to i128
  %164 = add nsw i32 %89, -1
  %165 = zext i32 %164 to i128
  %166 = shl i128 %163, %165
  br i1 %92, label %167, label %184

167:                                              ; preds = %162
  %168 = zext i32 %90 to i128
  %169 = zext i32 %91 to i128
  %170 = shl nuw i128 %168, %169
  %171 = or i128 %166, %170
  %172 = icmp eq i32 %159, 2
  %173 = trunc i128 %171 to i64
  %174 = lshr i128 %171, 64
  %175 = trunc i128 %174 to i64
  br i1 %172, label %176, label %195

176:                                              ; preds = %167
  %177 = add nsw i32 %91, -1
  %178 = zext i32 %177 to i128
  %179 = shl nuw i128 1, %178
  %180 = or i128 %171, %179
  %181 = trunc i128 %180 to i64
  %182 = lshr i128 %180, 64
  %183 = trunc i128 %182 to i64
  br label %195

184:                                              ; preds = %162
  %185 = icmp eq i32 %159, 2
  %186 = select i1 %185, i32 31, i32 30
  %187 = zext i32 %186 to i128
  %188 = add nsw i32 %89, -6
  %189 = zext i32 %188 to i128
  %190 = shl i128 %187, %189
  %191 = or i128 %190, %166
  %192 = trunc i128 %191 to i64
  %193 = lshr i128 %191, 64
  %194 = trunc i128 %193 to i64
  br label %195

195:                                              ; preds = %184, %176, %167
  %196 = phi i64 [ %181, %176 ], [ %173, %167 ], [ %192, %184 ]
  %197 = phi i64 [ %183, %176 ], [ %175, %167 ], [ %194, %184 ]
  %198 = lshr exact i32 %89, 3
  %199 = zext i64 %197 to i128
  %200 = shl nuw i128 %199, 64
  %201 = zext i64 %196 to i128
  %202 = or i128 %200, %201
  %203 = zext i32 %198 to i64
  br label %204

204:                                              ; preds = %204, %195
  %205 = phi i64 [ 0, %195 ], [ %210, %204 ]
  %206 = phi i128 [ %202, %195 ], [ %209, %204 ]
  %207 = trunc i128 %206 to i8
  %208 = getelementptr inbounds i8, ptr %0, i64 %205
  store i8 %207, ptr %208, align 1, !tbaa !26
  %209 = lshr i128 %206, 8
  %210 = add nuw nsw i64 %205, 1
  %211 = icmp eq i64 %210, %203
  br i1 %211, label %480, label %204
212:                                              ; preds = %158
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %9) #10
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %9, ptr noundef nonnull align 4 dereferenceable(5604) %7, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %10) #10
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %10, i8 0, i64 5604, i1 false), !alias.scope !31
  %213 = getelementptr inbounds i8, ptr %10, i64 4
  store i32 1, ptr %10, align 4, !tbaa !34, !alias.scope !31
  store i32 1, ptr %213, align 4, !tbaa !24, !alias.scope !31
  br i1 %100, label %214, label %383

214:                                              ; preds = %212
  %215 = getelementptr inbounds i8, ptr %7, i64 5604
  %216 = load i32, ptr %215, align 4, !tbaa !35
  %217 = getelementptr inbounds i8, ptr %8, i64 5604
  %218 = load i32, ptr %217, align 4, !tbaa !35
  %219 = tail call i32 @llvm.smin.i32(i32 %216, i32 %218)
  %220 = sub nsw i32 %216, %219
  call fastcc void @power(ptr noundef %9, i32 noundef %93, i32 noundef %220) #11
  %221 = sub nsw i32 %218, %219
  call fastcc void @power(ptr noundef %8, i32 noundef %93, i32 noundef %221) #11
  %222 = load i32, ptr %103, align 4, !tbaa !20
  %223 = icmp eq i32 %102, %222
  %224 = load i32, ptr %9, align 4, !tbaa !34
  %225 = load i32, ptr %8, align 4, !tbaa !34
  br i1 %223, label %226, label %267

226:                                              ; preds = %214
  %227 = tail call i32 @llvm.smax.i32(i32 %224, i32 %225)
  store i32 %227, ptr %9, align 4
  %228 = icmp sgt i32 %227, 0
  br i1 %228, label %229, label %459

229:                                              ; preds = %226
  %230 = getelementptr inbounds i8, ptr %9, i64 4
  %231 = getelementptr inbounds i8, ptr %8, i64 4
  %232 = sext i32 %224 to i64
  %233 = zext i32 %227 to i64
  %234 = sext i32 %225 to i64
  br label %237

235:                                              ; preds = %252
  %236 = icmp samesign ult i64 %255, 4294967296
  br i1 %236, label %459, label %261

237:                                              ; preds = %252, %229
  %238 = phi i64 [ 0, %229 ], [ %259, %252 ]
  %239 = phi i64 [ 0, %229 ], [ %258, %252 ]
  %240 = icmp slt i64 %238, %232
  br i1 %240, label %241, label %245

241:                                              ; preds = %237
  %242 = getelementptr inbounds [1400 x i32], ptr %230, i64 0, i64 %238
  %243 = load i32, ptr %242, align 4, !tbaa !24
  %244 = zext i32 %243 to i64
  br label %245

245:                                              ; preds = %241, %237
  %246 = phi i64 [ %244, %241 ], [ 0, %237 ]
  %247 = icmp slt i64 %238, %234
  br i1 %247, label %248, label %252

248:                                              ; preds = %245
  %249 = getelementptr inbounds [1400 x i32], ptr %231, i64 0, i64 %238
  %250 = load i32, ptr %249, align 4, !tbaa !24
  %251 = zext i32 %250 to i64
  br label %252

252:                                              ; preds = %248, %245
  %253 = phi i64 [ %251, %248 ], [ 0, %245 ]
  %254 = add nuw nsw i64 %246, %239
  %255 = add nuw nsw i64 %254, %253
  %256 = trunc i64 %255 to i32
  %257 = getelementptr inbounds [1400 x i32], ptr %230, i64 0, i64 %238
  store i32 %256, ptr %257, align 4, !tbaa !24
  %258 = lshr i64 %255, 32
  %259 = add nuw nsw i64 %238, 1
  %260 = icmp eq i64 %259, %233
  br i1 %260, label %235, label %237
261:                                              ; preds = %235
  %262 = icmp eq i32 %227, 1400
  br i1 %262, label %263, label %264

263:                                              ; preds = %261
  tail call void @llvm.trap()
  unreachable

264:                                              ; preds = %261
  %265 = add nuw nsw i32 %227, 1
  store i32 %265, ptr %9, align 4, !tbaa !34
  %266 = getelementptr inbounds [1400 x i32], ptr %230, i64 0, i64 %233
  store i32 1, ptr %266, align 4, !tbaa !24
  br label %459

267:                                              ; preds = %214
  %268 = icmp eq i32 %224, %225
  br i1 %268, label %269, label %275

269:                                              ; preds = %267
  %270 = getelementptr inbounds i8, ptr %9, i64 4
  %271 = getelementptr inbounds i8, ptr %8, i64 4
  %272 = icmp eq i32 %224, 0
  br i1 %272, label %291, label %273

273:                                              ; preds = %269
  %274 = sext i32 %224 to i64
  br label %280

275:                                              ; preds = %267
  %276 = icmp slt i32 %224, %225
  %277 = select i1 %276, i32 -1, i32 1
  br label %291

278:                                              ; preds = %280
  %279 = icmp eq i64 %282, 0
  br i1 %279, label %291, label %280
280:                                              ; preds = %273, %278
  %281 = phi i64 [ %274, %273 ], [ %282, %278 ]
  %282 = add nsw i64 %281, -1
  %283 = getelementptr inbounds [1400 x i32], ptr %270, i64 0, i64 %282
  %284 = load i32, ptr %283, align 4, !tbaa !24
  %285 = getelementptr inbounds [1400 x i32], ptr %271, i64 0, i64 %282
  %286 = load i32, ptr %285, align 4, !tbaa !24
  %287 = icmp eq i32 %284, %286
  br i1 %287, label %278, label %288
288:                                              ; preds = %280
  %289 = icmp ult i32 %284, %286
  %290 = select i1 %289, i32 -1, i32 1
  br label %291

291:                                              ; preds = %278, %269, %275, %288
  %292 = phi i32 [ %277, %275 ], [ %290, %288 ], [ 0, %269 ], [ 0, %278 ]
  %293 = icmp sgt i32 %292, -1
  br i1 %293, label %294, label %335

294:                                              ; preds = %291
  %295 = icmp sgt i32 %224, 0
  br i1 %295, label %296, label %301

296:                                              ; preds = %294
  %297 = getelementptr inbounds i8, ptr %8, i64 4
  %298 = getelementptr inbounds i8, ptr %9, i64 4
  %299 = zext i32 %224 to i64
  %300 = sext i32 %225 to i64
  br label %315

301:                                              ; preds = %323, %294
  %302 = getelementptr inbounds i8, ptr %9, i64 4
  %303 = icmp eq i32 %224, 0
  br i1 %303, label %378, label %304

304:                                              ; preds = %301
  %305 = sext i32 %224 to i64
  br label %306

306:                                              ; preds = %312, %304
  %307 = phi i64 [ %305, %304 ], [ %308, %312 ]
  %308 = add nsw i64 %307, -1
  %309 = getelementptr inbounds [1400 x i32], ptr %302, i64 0, i64 %308
  %310 = load i32, ptr %309, align 4, !tbaa !24
  %311 = icmp eq i32 %310, 0
  br i1 %311, label %312, label %378

312:                                              ; preds = %306
  %313 = trunc i64 %308 to i32
  store i32 %313, ptr %9, align 4, !tbaa !34
  %314 = icmp eq i64 %308, 0
  br i1 %314, label %378, label %306
315:                                              ; preds = %323, %296
  %316 = phi i64 [ 0, %296 ], [ %333, %323 ]
  %317 = phi i64 [ 0, %296 ], [ %332, %323 ]
  %318 = icmp slt i64 %316, %300
  br i1 %318, label %319, label %323

319:                                              ; preds = %315
  %320 = getelementptr inbounds [1400 x i32], ptr %297, i64 0, i64 %316
  %321 = load i32, ptr %320, align 4, !tbaa !24
  %322 = zext i32 %321 to i64
  br label %323

323:                                              ; preds = %319, %315
  %324 = phi i64 [ %322, %319 ], [ 0, %315 ]
  %325 = add nuw nsw i64 %324, %317
  %326 = getelementptr inbounds [1400 x i32], ptr %298, i64 0, i64 %316
  %327 = load i32, ptr %326, align 4, !tbaa !24
  %328 = zext i32 %327 to i64
  %329 = trunc i64 %325 to i32
  %330 = sub i32 %327, %329
  store i32 %330, ptr %326, align 4, !tbaa !24
  %331 = icmp samesign ugt i64 %325, %328
  %332 = zext i1 %331 to i64
  %333 = add nuw nsw i64 %316, 1
  %334 = icmp eq i64 %333, %299
  br i1 %334, label %301, label %315
335:                                              ; preds = %291
  %336 = icmp sgt i32 %225, 0
  br i1 %336, label %337, label %342

337:                                              ; preds = %335
  %338 = getelementptr inbounds i8, ptr %9, i64 4
  %339 = getelementptr inbounds i8, ptr %8, i64 4
  %340 = zext i32 %225 to i64
  %341 = sext i32 %224 to i64
  br label %356

342:                                              ; preds = %364, %335
  %343 = getelementptr inbounds i8, ptr %8, i64 4
  %344 = icmp eq i32 %225, 0
  br i1 %344, label %376, label %345

345:                                              ; preds = %342
  %346 = sext i32 %225 to i64
  br label %347

347:                                              ; preds = %353, %345
  %348 = phi i64 [ %346, %345 ], [ %349, %353 ]
  %349 = add nsw i64 %348, -1
  %350 = getelementptr inbounds [1400 x i32], ptr %343, i64 0, i64 %349
  %351 = load i32, ptr %350, align 4, !tbaa !24
  %352 = icmp eq i32 %351, 0
  br i1 %352, label %353, label %376

353:                                              ; preds = %347
  %354 = trunc i64 %349 to i32
  store i32 %354, ptr %8, align 4, !tbaa !34
  %355 = icmp eq i64 %349, 0
  br i1 %355, label %376, label %347
356:                                              ; preds = %364, %337
  %357 = phi i64 [ 0, %337 ], [ %374, %364 ]
  %358 = phi i64 [ 0, %337 ], [ %373, %364 ]
  %359 = icmp slt i64 %357, %341
  br i1 %359, label %360, label %364

360:                                              ; preds = %356
  %361 = getelementptr inbounds [1400 x i32], ptr %338, i64 0, i64 %357
  %362 = load i32, ptr %361, align 4, !tbaa !24
  %363 = zext i32 %362 to i64
  br label %364

364:                                              ; preds = %360, %356
  %365 = phi i64 [ %363, %360 ], [ 0, %356 ]
  %366 = add nuw nsw i64 %365, %358
  %367 = getelementptr inbounds [1400 x i32], ptr %339, i64 0, i64 %357
  %368 = load i32, ptr %367, align 4, !tbaa !24
  %369 = zext i32 %368 to i64
  %370 = trunc i64 %366 to i32
  %371 = sub i32 %368, %370
  store i32 %371, ptr %367, align 4, !tbaa !24
  %372 = icmp samesign ugt i64 %366, %369
  %373 = zext i1 %372 to i64
  %374 = add nuw nsw i64 %357, 1
  %375 = icmp eq i64 %374, %340
  br i1 %375, label %342, label %356
376:                                              ; preds = %347, %353, %342
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %9, ptr noundef nonnull align 4 dereferenceable(5604) %8, i64 5604, i1 false), !tbaa.struct !30
  %377 = load i32, ptr %103, align 4, !tbaa !20
  br label %378

378:                                              ; preds = %312, %306, %301, %376
  %379 = phi i32 [ %377, %376 ], [ %160, %301 ], [ %160, %306 ], [ %160, %312 ]
  %380 = load i32, ptr %9, align 4, !tbaa !34
  %381 = icmp eq i32 %380, 0
  %382 = select i1 %381, i32 0, i32 %379
  br label %459

383:                                              ; preds = %212
  %384 = icmp eq i32 %4, 2
  br i1 %384, label %385, label %453

385:                                              ; preds = %383
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %11) #10
  tail call void @llvm.experimental.noalias.scope.decl(metadata !40)
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %11, i8 0, i64 5604, i1 false), !alias.scope !40
  %386 = load i32, ptr %7, align 4, !tbaa !34, !noalias !40
  %387 = load i32, ptr %8, align 4, !tbaa !34, !noalias !40
  %388 = add nsw i32 %387, %386
  %389 = icmp sgt i32 %388, 1400
  br i1 %389, label %390, label %391

390:                                              ; preds = %385
  tail call void @llvm.trap()
  unreachable

391:                                              ; preds = %385
  store i32 %388, ptr %11, align 4, !tbaa !34, !alias.scope !40
  %392 = icmp sgt i32 %386, 0
  br i1 %392, label %393, label %407

393:                                              ; preds = %391
  %394 = icmp sgt i32 %387, 0
  %395 = getelementptr inbounds i8, ptr %7, i64 4
  %396 = getelementptr inbounds i8, ptr %8, i64 4
  %397 = getelementptr inbounds i8, ptr %11, i64 4
  %398 = sext i32 %387 to i64
  %399 = zext i32 %386 to i64
  %400 = zext i32 %387 to i64
  br label %401

401:                                              ; preds = %424, %393
  %402 = phi i64 [ 0, %393 ], [ %428, %424 ]
  br i1 %394, label %403, label %424

403:                                              ; preds = %401
  %404 = getelementptr inbounds [1400 x i32], ptr %395, i64 0, i64 %402
  %405 = load i32, ptr %404, align 4, !tbaa !24, !noalias !40
  %406 = zext i32 %405 to i64
  br label %430

407:                                              ; preds = %424, %391
  %408 = getelementptr inbounds i8, ptr %11, i64 4
  %409 = load i32, ptr %11, align 4, !tbaa !34, !alias.scope !40
  %410 = icmp eq i32 %409, 0
  br i1 %410, label %447, label %411

411:                                              ; preds = %407
  %412 = sext i32 %409 to i64
  br label %413

413:                                              ; preds = %419, %411
  %414 = phi i64 [ %412, %411 ], [ %415, %419 ]
  %415 = add nsw i64 %414, -1
  %416 = getelementptr inbounds [1400 x i32], ptr %408, i64 0, i64 %415
  %417 = load i32, ptr %416, align 4, !tbaa !24, !alias.scope !40
  %418 = icmp eq i32 %417, 0
  br i1 %418, label %419, label %447

419:                                              ; preds = %413
  %420 = trunc i64 %415 to i32
  store i32 %420, ptr %11, align 4, !tbaa !34, !alias.scope !40
  %421 = icmp eq i64 %415, 0
  br i1 %421, label %447, label %413
422:                                              ; preds = %430
  %423 = trunc i64 %444 to i32
  br label %424

424:                                              ; preds = %422, %401
  %425 = phi i32 [ 0, %401 ], [ %423, %422 ]
  %426 = add nsw i64 %402, %398
  %427 = getelementptr inbounds [1400 x i32], ptr %397, i64 0, i64 %426
  store i32 %425, ptr %427, align 4, !tbaa !24, !alias.scope !40
  %428 = add nuw nsw i64 %402, 1
  %429 = icmp eq i64 %428, %399
  br i1 %429, label %407, label %401
430:                                              ; preds = %430, %403
  %431 = phi i64 [ 0, %403 ], [ %445, %430 ]
  %432 = phi i64 [ 0, %403 ], [ %444, %430 ]
  %433 = getelementptr inbounds [1400 x i32], ptr %396, i64 0, i64 %431
  %434 = load i32, ptr %433, align 4, !tbaa !24, !noalias !40
  %435 = zext i32 %434 to i64
  %436 = mul nuw i64 %435, %406
  %437 = add nuw nsw i64 %431, %402
  %438 = getelementptr inbounds [1400 x i32], ptr %397, i64 0, i64 %437
  %439 = load i32, ptr %438, align 4, !tbaa !24, !alias.scope !40
  %440 = zext i32 %439 to i64
  %441 = add nuw nsw i64 %432, %440
  %442 = add nuw i64 %441, %436
  %443 = trunc i64 %442 to i32
  store i32 %443, ptr %438, align 4, !tbaa !24, !alias.scope !40
  %444 = lshr i64 %442, 32
  %445 = add nuw nsw i64 %431, 1
  %446 = icmp eq i64 %445, %400
  br i1 %446, label %422, label %430
447:                                              ; preds = %413, %419, %407
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %9, ptr noundef nonnull align 4 dereferenceable(5604) %11, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %11) #10
  %448 = getelementptr inbounds i8, ptr %7, i64 5604
  %449 = load i32, ptr %448, align 4, !tbaa !35
  %450 = getelementptr inbounds i8, ptr %8, i64 5604
  %451 = load i32, ptr %450, align 4, !tbaa !35
  %452 = add nsw i32 %451, %449
  br label %459

453:                                              ; preds = %383
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %10, ptr noundef nonnull align 4 dereferenceable(5604) %8, i64 5604, i1 false), !tbaa.struct !30
  %454 = getelementptr inbounds i8, ptr %7, i64 5604
  %455 = load i32, ptr %454, align 4, !tbaa !35
  %456 = getelementptr inbounds i8, ptr %8, i64 5604
  %457 = load i32, ptr %456, align 4, !tbaa !35
  %458 = sub nsw i32 %455, %457
  br label %459

459:                                              ; preds = %264, %235, %226, %447, %453, %378
  %460 = phi i32 [ %219, %378 ], [ %452, %447 ], [ %458, %453 ], [ %219, %226 ], [ %219, %235 ], [ %219, %264 ]
  %461 = phi i32 [ %382, %378 ], [ %160, %447 ], [ %160, %453 ], [ %160, %226 ], [ %160, %235 ], [ %160, %264 ]
  %462 = call fastcc { i64, i64 } @pack(ptr noundef %9, ptr noundef %10, i32 noundef %460, i32 noundef %461, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %6) #11
  %463 = extractvalue { i64, i64 } %462, 1
  %464 = extractvalue { i64, i64 } %462, 0
  %465 = lshr exact i32 %89, 3
  %466 = zext i64 %463 to i128
  %467 = shl nuw i128 %466, 64
  %468 = zext i64 %464 to i128
  %469 = or i128 %467, %468
  %470 = zext i32 %465 to i64
  br label %471

471:                                              ; preds = %471, %459
  %472 = phi i64 [ 0, %459 ], [ %477, %471 ]
  %473 = phi i128 [ %469, %459 ], [ %476, %471 ]
  %474 = trunc i128 %473 to i8
  %475 = getelementptr inbounds i8, ptr %0, i64 %472
  store i8 %474, ptr %475, align 1, !tbaa !26
  %476 = lshr i128 %473, 8
  %477 = add nuw nsw i64 %472, 1
  %478 = icmp eq i64 %477, %470
  br i1 %478, label %479, label %471
479:                                              ; preds = %471
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %10) #10
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %9) #10
  br label %480

480:                                              ; preds = %204, %479
  call void @llvm.lifetime.end.p0(i64 5616, ptr nonnull %8) #10
  call void @llvm.lifetime.end.p0(i64 5616, ptr nonnull %7) #10
  call void @llvm.lifetime.end.p0(i64 36, ptr nonnull %6) #10
  ret void
}

; Function Attrs: mustprogress nocallback nofree nosync nounwind willreturn memory(argmem: readwrite)
declare void @llvm.lifetime.start.p0(i64 immarg, ptr) #1

; Function Attrs: nounwind memory(argmem: readwrite, inaccessiblemem: write)
define internal fastcc void @decode(ptr noalias nonnull sret(%struct.tzrt_number) align 4 %0, ptr noundef readonly %1, ptr noundef readonly byval(%struct.tzrt_format) align 8 %2) unnamed_addr #2 {
  %4 = alloca %struct.tzrt_big, align 4
  %5 = alloca %struct.tzrt_big, align 4
  %6 = alloca %struct.tzrt_big, align 4
  %7 = alloca %struct.tzrt_big, align 4
  %8 = getelementptr inbounds i8, ptr %2, i64 24
  %9 = load i32, ptr %8, align 8, !tbaa !17
  %10 = add i32 %9, 7
  %11 = icmp ult i32 %10, 15
  br i1 %11, label %29, label %12

12:                                               ; preds = %3
  %13 = sdiv i32 %9, 8
  %14 = sext i32 %13 to i64
  br label %19

15:                                               ; preds = %19
  %16 = lshr i128 %23, 64
  %17 = trunc i128 %16 to i64
  %18 = trunc i128 %27 to i64
  br label %29

19:                                               ; preds = %19, %12
  %20 = phi i64 [ %14, %12 ], [ %22, %19 ]
  %21 = phi i128 [ 0, %12 ], [ %27, %19 ]
  %22 = add nsw i64 %20, -1
  %23 = shl i128 %21, 8
  %24 = getelementptr inbounds i8, ptr %1, i64 %22
  %25 = load i8, ptr %24, align 1, !tbaa !26
  %26 = zext i8 %25 to i128
  %27 = or i128 %23, %26
  %28 = icmp eq i64 %22, 0
  br i1 %28, label %15, label %19
29:                                               ; preds = %3, %15
  %30 = phi i64 [ 0, %3 ], [ %18, %15 ]
  %31 = phi i64 [ 0, %3 ], [ %17, %15 ]
  %32 = zext i64 %31 to i128
  %33 = shl nuw i128 %32, 64
  %34 = zext i64 %30 to i128
  %35 = or i128 %33, %34
  tail call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5616) %0, i8 0, i64 5616, i1 false)
  %36 = add nsw i32 %9, -1
  %37 = zext i32 %36 to i128
  %38 = lshr i128 %35, %37
  %39 = trunc i128 %38 to i32
  %40 = getelementptr inbounds i8, ptr %0, i64 5608
  store i32 %39, ptr %40, align 4, !tbaa !20
  %41 = getelementptr inbounds i8, ptr %2, i64 28
  %42 = load i32, ptr %41, align 4, !tbaa !18
  %43 = icmp eq i32 %42, 0
  br i1 %43, label %80, label %44

44:                                               ; preds = %29
  %45 = getelementptr inbounds i8, ptr %2, i64 32
  %46 = load i32, ptr %45, align 8, !tbaa !19
  %47 = and i32 %46, %39
  store i32 %47, ptr %40, align 4, !tbaa !20
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %4) #10
  %48 = icmp eq i32 %47, 0
  br i1 %48, label %60, label %49

49:                                               ; preds = %44
  %50 = sub i128 0, %35
  %51 = icmp eq i32 %9, 128
  %52 = zext i32 %9 to i128
  %53 = shl nsw i128 -1, %52
  %54 = xor i128 %53, -1
  %55 = select i1 %51, i128 -1, i128 %54
  %56 = and i128 %55, %50
  %57 = trunc i128 %56 to i64
  %58 = lshr i128 %56, 64
  %59 = trunc i128 %58 to i64
  br label %60

60:                                               ; preds = %44, %49
  %61 = phi i64 [ %57, %49 ], [ %30, %44 ]
  %62 = phi i64 [ %59, %49 ], [ %31, %44 ]
  %63 = zext i64 %62 to i128
  %64 = shl nuw i128 %63, 64
  %65 = zext i64 %61 to i128
  %66 = or i128 %64, %65
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %4, i8 0, i64 5604, i1 false), !alias.scope !46
  %67 = icmp eq i128 %66, 0
  br i1 %67, label %79, label %68

68:                                               ; preds = %60
  %69 = getelementptr inbounds i8, ptr %4, i64 4
  br label %70

70:                                               ; preds = %70, %68
  %71 = phi i128 [ %66, %68 ], [ %77, %70 ]
  %72 = trunc i128 %71 to i32
  %73 = load i32, ptr %4, align 4, !tbaa !34, !alias.scope !46
  %74 = add nsw i32 %73, 1
  store i32 %74, ptr %4, align 4, !tbaa !34, !alias.scope !46
  %75 = sext i32 %73 to i64
  %76 = getelementptr inbounds [1400 x i32], ptr %69, i64 0, i64 %75
  store i32 %72, ptr %76, align 4, !tbaa !24, !alias.scope !46
  %77 = lshr i128 %71, 32
  %78 = icmp ult i128 %71, 4294967296
  br i1 %78, label %79, label %70
79:                                               ; preds = %70, %60
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %0, ptr noundef nonnull align 4 dereferenceable(5604) %4, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %4) #10
  br label %320

80:                                               ; preds = %29
  %81 = icmp eq i32 %36, 128
  %82 = shl nsw i128 -1, %37
  %83 = xor i128 %82, -1
  %84 = select i1 %81, i128 -1, i128 %83
  %85 = and i128 %35, %84
  %86 = load i32, ptr %2, align 8, !tbaa !4
  %87 = icmp eq i32 %86, 2
  br i1 %87, label %88, label %132

88:                                               ; preds = %80
  %89 = getelementptr inbounds i8, ptr %2, i64 16
  %90 = load i32, ptr %89, align 8, !tbaa !15
  %91 = zext i32 %90 to i128
  %92 = lshr i128 %85, %91
  %93 = trunc i128 %92 to i32
  %94 = icmp eq i32 %90, 128
  %95 = shl nsw i128 -1, %91
  %96 = xor i128 %95, -1
  %97 = select i1 %94, i128 -1, i128 %96
  %98 = and i128 %97, %85
  %99 = getelementptr inbounds i8, ptr %2, i64 20
  %100 = load i32, ptr %99, align 4, !tbaa !16
  %101 = shl nsw i32 %100, 1
  %102 = or i32 %101, 1
  %103 = icmp eq i32 %102, %93
  br i1 %103, label %104, label %108

104:                                              ; preds = %88
  %105 = icmp eq i128 %98, 0
  %106 = select i1 %105, i32 1, i32 2
  %107 = getelementptr inbounds i8, ptr %0, i64 5612
  store i32 %106, ptr %107, align 4, !tbaa !23
  br label %320

108:                                              ; preds = %88
  %109 = icmp eq i32 %93, 0
  %110 = add i32 %100, %90
  %111 = sub i32 %93, %110
  %112 = getelementptr inbounds i8, ptr %2, i64 8
  %113 = load i32, ptr %112, align 8
  %114 = select i1 %109, i32 %113, i32 %111
  %115 = getelementptr inbounds i8, ptr %0, i64 5604
  store i32 %114, ptr %115, align 4, !tbaa !35
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %5) #10
  %116 = shl nuw i128 1, %91
  %117 = select i1 %109, i128 0, i128 %116
  %118 = or i128 %117, %98
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %5, i8 0, i64 5604, i1 false), !alias.scope !50
  %119 = icmp eq i128 %118, 0
  br i1 %119, label %131, label %120

120:                                              ; preds = %108
  %121 = getelementptr inbounds i8, ptr %5, i64 4
  br label %122

122:                                              ; preds = %122, %120
  %123 = phi i128 [ %118, %120 ], [ %129, %122 ]
  %124 = trunc i128 %123 to i32
  %125 = load i32, ptr %5, align 4, !tbaa !34, !alias.scope !50
  %126 = add nsw i32 %125, 1
  store i32 %126, ptr %5, align 4, !tbaa !34, !alias.scope !50
  %127 = sext i32 %125 to i64
  %128 = getelementptr inbounds [1400 x i32], ptr %121, i64 0, i64 %127
  store i32 %124, ptr %128, align 4, !tbaa !24, !alias.scope !50
  %129 = lshr i128 %123, 32
  %130 = icmp ult i128 %123, 4294967296
  br i1 %130, label %131, label %122
131:                                              ; preds = %122, %108
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %0, ptr noundef nonnull align 4 dereferenceable(5604) %5, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %5) #10
  br label %320

132:                                              ; preds = %80
  %133 = add nsw i32 %9, -6
  %134 = zext i32 %133 to i128
  %135 = lshr i128 %85, %134
  %136 = trunc i128 %135 to i32
  %137 = icmp sgt i32 %136, 29
  br i1 %137, label %138, label %142

138:                                              ; preds = %132
  %139 = icmp eq i32 %136, 30
  %140 = select i1 %139, i32 1, i32 2
  %141 = getelementptr inbounds i8, ptr %0, i64 5612
  store i32 %140, ptr %141, align 4, !tbaa !23
  br label %320

142:                                              ; preds = %132
  %143 = add nsw i32 %9, -3
  %144 = zext i32 %143 to i128
  %145 = lshr i128 %85, %144
  %146 = icmp eq i128 %145, 3
  %147 = icmp ne i32 %9, 128
  %148 = and i1 %147, %146
  %149 = getelementptr inbounds i8, ptr %2, i64 16
  %150 = load i32, ptr %149, align 8, !tbaa !15
  br i1 %148, label %151, label %177

151:                                              ; preds = %142
  %152 = add nsw i32 %150, -2
  %153 = zext i32 %152 to i128
  %154 = lshr i128 %85, %153
  %155 = xor i32 %150, -1
  %156 = add i32 %9, %155
  %157 = icmp eq i32 %156, 128
  %158 = zext i32 %156 to i128
  %159 = shl nsw i128 -1, %158
  %160 = trunc i128 %159 to i64
  %161 = xor i64 %160, -1
  %162 = zext i64 %161 to i128
  %163 = select i1 %157, i128 4294967295, i128 %162
  %164 = and i128 %163, %154
  %165 = trunc i128 %164 to i32
  %166 = icmp eq i32 %152, 128
  %167 = shl nsw i128 -1, %153
  %168 = xor i128 %167, -1
  %169 = select i1 %166, i128 -1, i128 %168
  %170 = and i128 %169, %85
  %171 = zext i32 %150 to i128
  %172 = shl nuw i128 1, %171
  %173 = or i128 %170, %172
  %174 = trunc i128 %173 to i64
  %175 = lshr i128 %173, 64
  %176 = trunc i128 %175 to i64
  br label %189

177:                                              ; preds = %142
  %178 = zext i32 %150 to i128
  %179 = lshr i128 %85, %178
  %180 = trunc i128 %179 to i32
  %181 = icmp eq i32 %150, 128
  %182 = shl nsw i128 -1, %178
  %183 = xor i128 %182, -1
  %184 = select i1 %181, i128 -1, i128 %183
  %185 = and i128 %184, %85
  %186 = trunc i128 %185 to i64
  %187 = lshr i128 %185, 64
  %188 = trunc i128 %187 to i64
  br label %189

189:                                              ; preds = %177, %151
  %190 = phi i64 [ %186, %177 ], [ %174, %151 ]
  %191 = phi i64 [ %188, %177 ], [ %176, %151 ]
  %192 = phi i32 [ %180, %177 ], [ %165, %151 ]
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %6) #10
  %193 = zext i64 %191 to i128
  %194 = shl nuw i128 %193, 64
  %195 = zext i64 %190 to i128
  %196 = or i128 %194, %195
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %6, i8 0, i64 5604, i1 false), !alias.scope !53
  %197 = icmp eq i128 %196, 0
  br i1 %197, label %209, label %198

198:                                              ; preds = %189
  %199 = getelementptr inbounds i8, ptr %6, i64 4
  br label %200

200:                                              ; preds = %200, %198
  %201 = phi i128 [ %196, %198 ], [ %207, %200 ]
  %202 = trunc i128 %201 to i32
  %203 = load i32, ptr %6, align 4, !tbaa !34, !alias.scope !53
  %204 = add nsw i32 %203, 1
  store i32 %204, ptr %6, align 4, !tbaa !34, !alias.scope !53
  %205 = sext i32 %203 to i64
  %206 = getelementptr inbounds [1400 x i32], ptr %199, i64 0, i64 %205
  store i32 %202, ptr %206, align 4, !tbaa !24, !alias.scope !53
  %207 = lshr i128 %201, 32
  %208 = icmp ult i128 %201, 4294967296
  br i1 %208, label %209, label %200
209:                                              ; preds = %200, %189
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %0, ptr noundef nonnull align 4 dereferenceable(5604) %6, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %6) #10
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %7) #10
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %7, i8 0, i64 5604, i1 false), !alias.scope !56
  %210 = getelementptr inbounds i8, ptr %7, i64 4
  store i32 1, ptr %7, align 4, !tbaa !34, !alias.scope !56
  store i32 1, ptr %210, align 4, !tbaa !24, !alias.scope !56
  %211 = getelementptr inbounds i8, ptr %2, i64 4
  %212 = load i32, ptr %211, align 4, !tbaa !12
  %213 = icmp sgt i32 %212, 8
  br i1 %213, label %217, label %214

214:                                              ; preds = %246, %209
  %215 = phi i32 [ %212, %209 ], [ %247, %246 ]
  %216 = icmp sgt i32 %215, 0
  br i1 %216, label %249, label %281

217:                                              ; preds = %209, %246
  %218 = phi i32 [ %247, %246 ], [ %212, %209 ]
  %219 = load i32, ptr %7, align 4, !tbaa !34
  %220 = icmp sgt i32 %219, 0
  br i1 %220, label %221, label %223

221:                                              ; preds = %217
  %222 = zext i32 %219 to i64
  br label %226

223:                                              ; preds = %226, %217
  %224 = phi i64 [ 0, %217 ], [ %235, %226 ]
  %225 = icmp eq i64 %224, 0
  br i1 %225, label %246, label %238

226:                                              ; preds = %226, %221
  %227 = phi i64 [ 0, %221 ], [ %236, %226 ]
  %228 = phi i64 [ 0, %221 ], [ %235, %226 ]
  %229 = getelementptr inbounds [1400 x i32], ptr %210, i64 0, i64 %227
  %230 = load i32, ptr %229, align 4, !tbaa !24
  %231 = zext i32 %230 to i64
  %232 = mul nuw nsw i64 %231, 1000000000
  %233 = add nuw nsw i64 %232, %228
  %234 = trunc i64 %233 to i32
  store i32 %234, ptr %229, align 4, !tbaa !24
  %235 = lshr i64 %233, 32
  %236 = add nuw nsw i64 %227, 1
  %237 = icmp eq i64 %236, %222
  br i1 %237, label %223, label %226
238:                                              ; preds = %223
  %239 = icmp eq i32 %219, 1400
  br i1 %239, label %240, label %241

240:                                              ; preds = %238
  tail call void @llvm.trap()
  unreachable

241:                                              ; preds = %238
  %242 = trunc i64 %224 to i32
  %243 = add nsw i32 %219, 1
  store i32 %243, ptr %7, align 4, !tbaa !34
  %244 = sext i32 %219 to i64
  %245 = getelementptr inbounds [1400 x i32], ptr %210, i64 0, i64 %244
  store i32 %242, ptr %245, align 4, !tbaa !24
  br label %246

246:                                              ; preds = %241, %223
  %247 = add nsw i32 %218, -9
  %248 = icmp sgt i32 %218, 17
  br i1 %248, label %217, label %214
249:                                              ; preds = %214, %279
  %250 = phi i32 [ %251, %279 ], [ %215, %214 ]
  %251 = add nsw i32 %250, -1
  %252 = load i32, ptr %7, align 4, !tbaa !34
  %253 = icmp sgt i32 %252, 0
  br i1 %253, label %254, label %256

254:                                              ; preds = %249
  %255 = zext i32 %252 to i64
  br label %259

256:                                              ; preds = %259, %249
  %257 = phi i64 [ 0, %249 ], [ %268, %259 ]
  %258 = icmp eq i64 %257, 0
  br i1 %258, label %279, label %271

259:                                              ; preds = %259, %254
  %260 = phi i64 [ 0, %254 ], [ %269, %259 ]
  %261 = phi i64 [ 0, %254 ], [ %268, %259 ]
  %262 = getelementptr inbounds [1400 x i32], ptr %210, i64 0, i64 %260
  %263 = load i32, ptr %262, align 4, !tbaa !24
  %264 = zext i32 %263 to i64
  %265 = mul nuw nsw i64 %264, 10
  %266 = add nuw nsw i64 %265, %261
  %267 = trunc i64 %266 to i32
  store i32 %267, ptr %262, align 4, !tbaa !24
  %268 = lshr i64 %266, 32
  %269 = add nuw nsw i64 %260, 1
  %270 = icmp eq i64 %269, %255
  br i1 %270, label %256, label %259
271:                                              ; preds = %256
  %272 = icmp eq i32 %252, 1400
  br i1 %272, label %273, label %274

273:                                              ; preds = %271
  tail call void @llvm.trap()
  unreachable

274:                                              ; preds = %271
  %275 = trunc i64 %257 to i32
  %276 = add nsw i32 %252, 1
  store i32 %276, ptr %7, align 4, !tbaa !34
  %277 = sext i32 %252 to i64
  %278 = getelementptr inbounds [1400 x i32], ptr %210, i64 0, i64 %277
  store i32 %275, ptr %278, align 4, !tbaa !24
  br label %279

279:                                              ; preds = %274, %256
  %280 = icmp sgt i32 %250, 1
  br i1 %280, label %249, label %281
281:                                              ; preds = %279, %214
  %282 = load i32, ptr %0, align 4, !tbaa !34
  %283 = load i32, ptr %7, align 4, !tbaa !34
  %284 = icmp eq i32 %282, %283
  br i1 %284, label %285, label %290

285:                                              ; preds = %281
  %286 = getelementptr inbounds i8, ptr %0, i64 4
  %287 = icmp eq i32 %282, 0
  br i1 %287, label %306, label %288

288:                                              ; preds = %285
  %289 = sext i32 %282 to i64
  br label %295

290:                                              ; preds = %281
  %291 = icmp slt i32 %282, %283
  %292 = select i1 %291, i32 -1, i32 1
  br label %306

293:                                              ; preds = %295
  %294 = icmp eq i64 %297, 0
  br i1 %294, label %306, label %295
295:                                              ; preds = %288, %293
  %296 = phi i64 [ %289, %288 ], [ %297, %293 ]
  %297 = add nsw i64 %296, -1
  %298 = getelementptr inbounds [1400 x i32], ptr %286, i64 0, i64 %297
  %299 = load i32, ptr %298, align 4, !tbaa !24
  %300 = getelementptr inbounds [1400 x i32], ptr %210, i64 0, i64 %297
  %301 = load i32, ptr %300, align 4, !tbaa !24
  %302 = icmp eq i32 %299, %301
  br i1 %302, label %293, label %303
303:                                              ; preds = %295
  %304 = icmp ult i32 %299, %301
  %305 = select i1 %304, i32 -1, i32 1
  br label %306

306:                                              ; preds = %293, %285, %290, %303
  %307 = phi i32 [ %292, %290 ], [ %305, %303 ], [ 0, %285 ], [ 0, %293 ]
  %308 = icmp sgt i32 %307, -1
  br i1 %308, label %314, label %309

309:                                              ; preds = %306
  %310 = icmp eq i32 %9, 128
  %311 = and i128 %85, -42535295865117307932921825928971026432
  %312 = icmp eq i128 %311, 127605887595351923798765477786913079296
  %313 = select i1 %310, i1 %312, i1 false
  br i1 %313, label %314, label %315

314:                                              ; preds = %309, %306
  tail call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %0, i8 0, i64 5604, i1 false)
  br label %315

315:                                              ; preds = %314, %309
  %316 = getelementptr inbounds i8, ptr %2, i64 20
  %317 = load i32, ptr %316, align 4, !tbaa !16
  %318 = sub nsw i32 %192, %317
  %319 = getelementptr inbounds i8, ptr %0, i64 5604
  store i32 %318, ptr %319, align 4, !tbaa !35
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %7) #10
  br label %320

320:                                              ; preds = %131, %104, %315, %138, %79
  ret void
}

; Function Attrs: mustprogress nocallback nofree nounwind willreturn memory(argmem: readwrite)
declare void @llvm.memcpy.p0.p0.i64(ptr noalias writeonly, ptr noalias readonly, i64, i1 immarg) #3

; Function Attrs: mustprogress nocallback nofree nosync nounwind willreturn memory(argmem: readwrite)
declare void @llvm.lifetime.end.p0(i64 immarg, ptr) #1

; Function Attrs: nounwind memory(argmem: readwrite, inaccessiblemem: write)
define internal fastcc void @power(ptr noundef nonnull %0, i32 noundef %1, i32 noundef %2) unnamed_addr #2 {
  %4 = icmp eq i32 %1, 2
  br i1 %4, label %9, label %5

5:                                                ; preds = %3
  %6 = icmp sgt i32 %2, 8
  br i1 %6, label %7, label %70

7:                                                ; preds = %5
  %8 = getelementptr inbounds i8, ptr %0, i64 4
  br label %75

9:                                                ; preds = %3
  %10 = load i32, ptr %0, align 4, !tbaa !34
  %11 = icmp ne i32 %10, 0
  %12 = icmp ne i32 %2, 0
  %13 = and i1 %12, %11
  br i1 %13, label %14, label %139

14:                                               ; preds = %9
  %15 = sdiv i32 %2, 32
  %16 = srem i32 %2, 32
  %17 = add nsw i32 %10, %15
  %18 = icmp sgt i32 %17, 1399
  br i1 %18, label %23, label %19

19:                                               ; preds = %14
  %20 = getelementptr inbounds i8, ptr %0, i64 4
  %21 = sext i32 %10 to i64
  %22 = sext i32 %15 to i64
  br label %28

23:                                               ; preds = %14
  tail call void @llvm.trap()
  unreachable

24:                                               ; preds = %28
  %25 = icmp sgt i32 %2, 31
  br i1 %25, label %26, label %36

26:                                               ; preds = %24
  %27 = zext i32 %15 to i64
  br label %42

28:                                               ; preds = %28, %19
  %29 = phi i64 [ %21, %19 ], [ %30, %28 ]
  %30 = add nsw i64 %29, -1
  %31 = getelementptr inbounds [1400 x i32], ptr %20, i64 0, i64 %30
  %32 = load i32, ptr %31, align 4, !tbaa !24
  %33 = add nsw i64 %30, %22
  %34 = getelementptr inbounds [1400 x i32], ptr %20, i64 0, i64 %33
  store i32 %32, ptr %34, align 4, !tbaa !24
  %35 = icmp eq i64 %30, 0
  br i1 %35, label %24, label %28
36:                                               ; preds = %42, %24
  %37 = load i32, ptr %0, align 4, !tbaa !34
  %38 = add nsw i32 %37, %15
  store i32 %38, ptr %0, align 4, !tbaa !34
  %39 = icmp sgt i32 %37, 0
  br i1 %39, label %40, label %47

40:                                               ; preds = %36
  %41 = zext i32 %16 to i64
  br label %51

42:                                               ; preds = %42, %26
  %43 = phi i64 [ 0, %26 ], [ %45, %42 ]
  %44 = getelementptr inbounds [1400 x i32], ptr %20, i64 0, i64 %43
  store i32 0, ptr %44, align 4, !tbaa !24
  %45 = add nuw nsw i64 %43, 1
  %46 = icmp eq i64 %45, %27
  br i1 %46, label %36, label %42
47:                                               ; preds = %51, %36
  %48 = phi i64 [ 0, %36 ], [ %60, %51 ]
  %49 = phi i32 [ %38, %36 ], [ %62, %51 ]
  %50 = icmp eq i64 %48, 0
  br i1 %50, label %139, label %65

51:                                               ; preds = %51, %40
  %52 = phi i64 [ %22, %40 ], [ %61, %51 ]
  %53 = phi i64 [ 0, %40 ], [ %60, %51 ]
  %54 = getelementptr inbounds [1400 x i32], ptr %20, i64 0, i64 %52
  %55 = load i32, ptr %54, align 4, !tbaa !24
  %56 = zext i32 %55 to i64
  %57 = shl i64 %56, %41
  %58 = or i64 %57, %53
  %59 = trunc i64 %58 to i32
  store i32 %59, ptr %54, align 4, !tbaa !24
  %60 = lshr i64 %57, 32
  %61 = add nsw i64 %52, 1
  %62 = load i32, ptr %0, align 4, !tbaa !34
  %63 = sext i32 %62 to i64
  %64 = icmp slt i64 %61, %63
  br i1 %64, label %51, label %47
65:                                               ; preds = %47
  %66 = trunc i64 %48 to i32
  %67 = add nsw i32 %49, 1
  store i32 %67, ptr %0, align 4, !tbaa !34
  %68 = sext i32 %49 to i64
  %69 = getelementptr inbounds [1400 x i32], ptr %20, i64 0, i64 %68
  store i32 %66, ptr %69, align 4, !tbaa !24
  br label %139

70:                                               ; preds = %104, %5
  %71 = phi i32 [ %2, %5 ], [ %105, %104 ]
  %72 = icmp sgt i32 %71, 0
  br i1 %72, label %73, label %139

73:                                               ; preds = %70
  %74 = getelementptr inbounds i8, ptr %0, i64 4
  br label %107

75:                                               ; preds = %7, %104
  %76 = phi i32 [ %2, %7 ], [ %105, %104 ]
  %77 = load i32, ptr %0, align 4, !tbaa !34
  %78 = icmp sgt i32 %77, 0
  br i1 %78, label %79, label %81

79:                                               ; preds = %75
  %80 = zext i32 %77 to i64
  br label %84

81:                                               ; preds = %84, %75
  %82 = phi i64 [ 0, %75 ], [ %93, %84 ]
  %83 = icmp eq i64 %82, 0
  br i1 %83, label %104, label %96

84:                                               ; preds = %84, %79
  %85 = phi i64 [ 0, %79 ], [ %94, %84 ]
  %86 = phi i64 [ 0, %79 ], [ %93, %84 ]
  %87 = getelementptr inbounds [1400 x i32], ptr %8, i64 0, i64 %85
  %88 = load i32, ptr %87, align 4, !tbaa !24
  %89 = zext i32 %88 to i64
  %90 = mul nuw nsw i64 %89, 1000000000
  %91 = add nuw nsw i64 %90, %86
  %92 = trunc i64 %91 to i32
  store i32 %92, ptr %87, align 4, !tbaa !24
  %93 = lshr i64 %91, 32
  %94 = add nuw nsw i64 %85, 1
  %95 = icmp eq i64 %94, %80
  br i1 %95, label %81, label %84
96:                                               ; preds = %81
  %97 = icmp eq i32 %77, 1400
  br i1 %97, label %98, label %99

98:                                               ; preds = %96
  tail call void @llvm.trap()
  unreachable

99:                                               ; preds = %96
  %100 = trunc i64 %82 to i32
  %101 = add nsw i32 %77, 1
  store i32 %101, ptr %0, align 4, !tbaa !34
  %102 = sext i32 %77 to i64
  %103 = getelementptr inbounds [1400 x i32], ptr %8, i64 0, i64 %102
  store i32 %100, ptr %103, align 4, !tbaa !24
  br label %104

104:                                              ; preds = %81, %99
  %105 = add nsw i32 %76, -9
  %106 = icmp sgt i32 %76, 17
  br i1 %106, label %75, label %70
107:                                              ; preds = %73, %137
  %108 = phi i32 [ %71, %73 ], [ %109, %137 ]
  %109 = add nsw i32 %108, -1
  %110 = load i32, ptr %0, align 4, !tbaa !34
  %111 = icmp sgt i32 %110, 0
  br i1 %111, label %112, label %114

112:                                              ; preds = %107
  %113 = zext i32 %110 to i64
  br label %117

114:                                              ; preds = %117, %107
  %115 = phi i64 [ 0, %107 ], [ %126, %117 ]
  %116 = icmp eq i64 %115, 0
  br i1 %116, label %137, label %129

117:                                              ; preds = %117, %112
  %118 = phi i64 [ 0, %112 ], [ %127, %117 ]
  %119 = phi i64 [ 0, %112 ], [ %126, %117 ]
  %120 = getelementptr inbounds [1400 x i32], ptr %74, i64 0, i64 %118
  %121 = load i32, ptr %120, align 4, !tbaa !24
  %122 = zext i32 %121 to i64
  %123 = mul nuw nsw i64 %122, 10
  %124 = add nuw nsw i64 %123, %119
  %125 = trunc i64 %124 to i32
  store i32 %125, ptr %120, align 4, !tbaa !24
  %126 = lshr i64 %124, 32
  %127 = add nuw nsw i64 %118, 1
  %128 = icmp eq i64 %127, %113
  br i1 %128, label %114, label %117
129:                                              ; preds = %114
  %130 = icmp eq i32 %110, 1400
  br i1 %130, label %131, label %132

131:                                              ; preds = %129
  tail call void @llvm.trap()
  unreachable

132:                                              ; preds = %129
  %133 = trunc i64 %115 to i32
  %134 = add nsw i32 %110, 1
  store i32 %134, ptr %0, align 4, !tbaa !34
  %135 = sext i32 %110 to i64
  %136 = getelementptr inbounds [1400 x i32], ptr %74, i64 0, i64 %135
  store i32 %133, ptr %136, align 4, !tbaa !24
  br label %137

137:                                              ; preds = %114, %132
  %138 = icmp sgt i32 %108, 1
  br i1 %138, label %107, label %139
139:                                              ; preds = %137, %70, %65, %47, %9
  ret void
}

; Function Attrs: nounwind memory(argmem: readwrite, inaccessiblemem: readwrite)
define internal fastcc { i64, i64 } @pack(ptr noundef nonnull readonly %0, ptr noundef nonnull readonly %1, i32 noundef %2, i32 noundef %3, ptr noundef readonly byval(%struct.tzrt_format) align 8 %4) unnamed_addr #0 {
  %6 = alloca %struct.tzrt_big, align 4
  %7 = alloca %struct.tzrt_big, align 4
  %8 = alloca %struct.tzrt_big, align 4
  %9 = alloca %struct.tzrt_big, align 4
  %10 = alloca %struct.tzrt_big, align 4
  %11 = alloca %struct.tzrt_big, align 4
  %12 = sext i32 %3 to i128
  %13 = getelementptr inbounds i8, ptr %4, i64 24
  %14 = load i32, ptr %13, align 8, !tbaa !17
  %15 = add nsw i32 %14, -1
  %16 = zext i32 %15 to i128
  %17 = shl i128 %12, %16
  %18 = load i32, ptr %0, align 4, !tbaa !34
  %19 = icmp eq i32 %18, 0
  %20 = load i32, ptr %4, align 8, !tbaa !4
  br i1 %19, label %21, label %45

21:                                               ; preds = %5
  %22 = icmp eq i32 %20, 10
  br i1 %22, label %23, label %39

23:                                               ; preds = %21
  %24 = getelementptr inbounds i8, ptr %4, i64 8
  %25 = load i32, ptr %24, align 8, !tbaa !13
  %26 = icmp slt i32 %2, %25
  %27 = getelementptr inbounds i8, ptr %4, i64 12
  %28 = load i32, ptr %27, align 4
  %29 = tail call i32 @llvm.smin.i32(i32 %2, i32 %28)
  %30 = select i1 %26, i32 %25, i32 %29
  %31 = getelementptr inbounds i8, ptr %4, i64 20
  %32 = load i32, ptr %31, align 4, !tbaa !16
  %33 = add nsw i32 %30, %32
  %34 = sext i32 %33 to i128
  %35 = getelementptr inbounds i8, ptr %4, i64 16
  %36 = load i32, ptr %35, align 8, !tbaa !15
  %37 = zext i32 %36 to i128
  %38 = shl i128 %34, %37
  br label %39

39:                                               ; preds = %21, %23
  %40 = phi i128 [ %38, %23 ], [ 0, %21 ]
  %41 = or i128 %40, %17
  %42 = trunc i128 %41 to i64
  %43 = lshr i128 %41, 64
  %44 = trunc i128 %43 to i64
  br label %435

45:                                               ; preds = %5
  %46 = tail call fastcc i32 @magnitude(ptr noundef %0, ptr noundef %1, i32 noundef %20) #11
  %47 = getelementptr inbounds i8, ptr %4, i64 4
  %48 = load i32, ptr %47, align 4, !tbaa !12
  %49 = add i32 %2, 1
  %50 = add i32 %49, %46
  %51 = sub i32 %50, %48
  %52 = getelementptr inbounds i8, ptr %4, i64 8
  %53 = load i32, ptr %52, align 8, !tbaa !13
  %54 = tail call i32 @llvm.smax.i32(i32 %51, i32 %53)
  %55 = getelementptr inbounds i8, ptr %4, i64 12
  %56 = load i32, ptr %55, align 4, !tbaa !14
  %57 = icmp sgt i32 %54, %56
  br i1 %57, label %58, label %82

58:                                               ; preds = %45
  %59 = icmp eq i32 %20, 2
  br i1 %59, label %60, label %74

60:                                               ; preds = %58
  %61 = getelementptr inbounds i8, ptr %4, i64 20
  %62 = load i32, ptr %61, align 4, !tbaa !16
  %63 = shl nsw i32 %62, 1
  %64 = or i32 %63, 1
  %65 = sext i32 %64 to i128
  %66 = getelementptr inbounds i8, ptr %4, i64 16
  %67 = load i32, ptr %66, align 8, !tbaa !15
  %68 = zext i32 %67 to i128
  %69 = shl i128 %65, %68
  %70 = or i128 %69, %17
  %71 = trunc i128 %70 to i64
  %72 = lshr i128 %70, 64
  %73 = trunc i128 %72 to i64
  br label %435

74:                                               ; preds = %58
  %75 = add nsw i32 %14, -6
  %76 = zext i32 %75 to i128
  %77 = shl i128 30, %76
  %78 = or i128 %17, %77
  %79 = trunc i128 %78 to i64
  %80 = lshr i128 %78, 64
  %81 = trunc i128 %80 to i64
  br label %435

82:                                               ; preds = %45
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %9) #10
  %83 = sub nsw i32 %2, %54
  tail call void @llvm.experimental.noalias.scope.decl(metadata !65)
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %6) #10, !noalias !65
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %6, ptr noundef nonnull readonly align 4 dereferenceable(5604) %0, i64 5604, i1 false), !tbaa.struct !30, !noalias !65
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %7) #10, !noalias !65
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %7, ptr noundef nonnull readonly align 4 dereferenceable(5604) %1, i64 5604, i1 false), !tbaa.struct !30, !noalias !65
  %84 = icmp sgt i32 %83, -1
  br i1 %84, label %85, label %86

85:                                               ; preds = %82
  call fastcc void @power(ptr noundef %6, i32 noundef %20, i32 noundef %83) #11, !noalias !65
  br label %88

86:                                               ; preds = %82
  %87 = sub nsw i32 0, %83
  call fastcc void @power(ptr noundef %7, i32 noundef %20, i32 noundef %87) #11, !noalias !65
  br label %88

88:                                               ; preds = %86, %85
  call fastcc void @divide(ptr nonnull sret(%struct.tzrt_big) align 4 %9, ptr noundef %6, ptr noundef %7) #11
  %89 = load i32, ptr %6, align 4, !tbaa !34, !noalias !65
  %90 = icmp sgt i32 %89, 0
  br i1 %90, label %91, label %94

91:                                               ; preds = %88
  %92 = getelementptr inbounds i8, ptr %6, i64 4
  %93 = zext i32 %89 to i64
  br label %97

94:                                               ; preds = %97, %88
  %95 = phi i64 [ 0, %88 ], [ %106, %97 ]
  %96 = icmp eq i64 %95, 0
  br i1 %96, label %118, label %109

97:                                               ; preds = %97, %91
  %98 = phi i64 [ 0, %91 ], [ %107, %97 ]
  %99 = phi i64 [ 0, %91 ], [ %106, %97 ]
  %100 = getelementptr inbounds [1400 x i32], ptr %92, i64 0, i64 %98
  %101 = load i32, ptr %100, align 4, !tbaa !24, !noalias !65
  %102 = zext i32 %101 to i64
  %103 = shl nuw nsw i64 %102, 1
  %104 = add nuw nsw i64 %103, %99
  %105 = trunc i64 %104 to i32
  store i32 %105, ptr %100, align 4, !tbaa !24, !noalias !65
  %106 = lshr i64 %104, 32
  %107 = add nuw nsw i64 %98, 1
  %108 = icmp eq i64 %107, %93
  br i1 %108, label %94, label %97
109:                                              ; preds = %94
  %110 = icmp eq i32 %89, 1400
  br i1 %110, label %111, label %112

111:                                              ; preds = %109
  tail call void @llvm.trap()
  unreachable

112:                                              ; preds = %109
  %113 = trunc i64 %95 to i32
  %114 = getelementptr inbounds i8, ptr %6, i64 4
  %115 = add nsw i32 %89, 1
  store i32 %115, ptr %6, align 4, !tbaa !34, !noalias !65
  %116 = sext i32 %89 to i64
  %117 = getelementptr inbounds [1400 x i32], ptr %114, i64 0, i64 %116
  store i32 %113, ptr %117, align 4, !tbaa !24, !noalias !65
  br label %118

118:                                              ; preds = %112, %94
  %119 = load i32, ptr %6, align 4, !tbaa !34, !noalias !65
  %120 = load i32, ptr %7, align 4, !tbaa !34, !noalias !65
  %121 = icmp eq i32 %119, %120
  br i1 %121, label %122, label %128

122:                                              ; preds = %118
  %123 = getelementptr inbounds i8, ptr %6, i64 4
  %124 = getelementptr inbounds i8, ptr %7, i64 4
  %125 = icmp eq i32 %119, 0
  br i1 %125, label %144, label %126

126:                                              ; preds = %122
  %127 = sext i32 %119 to i64
  br label %133

128:                                              ; preds = %118
  %129 = icmp slt i32 %119, %120
  %130 = select i1 %129, i32 -1, i32 1
  br label %144

131:                                              ; preds = %133
  %132 = icmp eq i64 %135, 0
  br i1 %132, label %144, label %133
133:                                              ; preds = %126, %131
  %134 = phi i64 [ %127, %126 ], [ %135, %131 ]
  %135 = add nsw i64 %134, -1
  %136 = getelementptr inbounds [1400 x i32], ptr %123, i64 0, i64 %135
  %137 = load i32, ptr %136, align 4, !tbaa !24, !noalias !65
  %138 = getelementptr inbounds [1400 x i32], ptr %124, i64 0, i64 %135
  %139 = load i32, ptr %138, align 4, !tbaa !24, !noalias !65
  %140 = icmp eq i32 %137, %139
  br i1 %140, label %131, label %141
141:                                              ; preds = %133
  %142 = icmp ult i32 %137, %139
  %143 = select i1 %142, i32 -1, i32 1
  br label %144

144:                                              ; preds = %131, %122, %141, %128
  %145 = phi i32 [ %130, %128 ], [ %143, %141 ], [ 0, %122 ], [ 0, %131 ]
  %146 = icmp sgt i32 %145, 0
  br i1 %146, label %157, label %147

147:                                              ; preds = %144
  %148 = icmp eq i32 %145, 0
  %149 = load i32, ptr %9, align 4, !alias.scope !65
  %150 = icmp ne i32 %149, 0
  %151 = select i1 %148, i1 %150, i1 false
  br i1 %151, label %152, label %197

152:                                              ; preds = %147
  %153 = getelementptr inbounds i8, ptr %9, i64 4
  %154 = load i32, ptr %153, align 4, !tbaa !24, !alias.scope !65
  %155 = and i32 %154, 1
  %156 = icmp eq i32 %155, 0
  br i1 %156, label %197, label %157

157:                                              ; preds = %152, %144
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %8) #10, !noalias !65
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %8, i8 0, i64 5604, i1 false), !alias.scope !68, !noalias !65
  %158 = getelementptr inbounds i8, ptr %8, i64 4
  store i32 1, ptr %8, align 4, !tbaa !34, !alias.scope !68, !noalias !65
  store i32 1, ptr %158, align 4, !tbaa !24, !alias.scope !68, !noalias !65
  %159 = load i32, ptr %9, align 4, !tbaa !34, !alias.scope !65
  %160 = tail call i32 @llvm.smax.i32(i32 %159, i32 1)
  store i32 %160, ptr %9, align 4, !alias.scope !65
  %161 = getelementptr inbounds i8, ptr %9, i64 4
  %162 = sext i32 %159 to i64
  %163 = zext i32 %160 to i64
  br label %166

164:                                              ; preds = %181
  %165 = icmp samesign ult i64 %184, 4294967296
  br i1 %165, label %196, label %190

166:                                              ; preds = %181, %157
  %167 = phi i64 [ 0, %157 ], [ %188, %181 ]
  %168 = phi i64 [ 0, %157 ], [ %187, %181 ]
  %169 = icmp slt i64 %167, %162
  br i1 %169, label %170, label %174

170:                                              ; preds = %166
  %171 = getelementptr inbounds [1400 x i32], ptr %161, i64 0, i64 %167
  %172 = load i32, ptr %171, align 4, !tbaa !24, !alias.scope !65
  %173 = zext i32 %172 to i64
  br label %174

174:                                              ; preds = %170, %166
  %175 = phi i64 [ %173, %170 ], [ 0, %166 ]
  %176 = icmp eq i64 %167, 0
  br i1 %176, label %177, label %181

177:                                              ; preds = %174
  %178 = getelementptr inbounds [1400 x i32], ptr %158, i64 0, i64 %167
  %179 = load i32, ptr %178, align 4, !tbaa !24, !noalias !65
  %180 = zext i32 %179 to i64
  br label %181

181:                                              ; preds = %177, %174
  %182 = phi i64 [ %180, %177 ], [ 0, %174 ]
  %183 = add nuw nsw i64 %175, %168
  %184 = add nuw nsw i64 %183, %182
  %185 = trunc i64 %184 to i32
  %186 = getelementptr inbounds [1400 x i32], ptr %161, i64 0, i64 %167
  store i32 %185, ptr %186, align 4, !tbaa !24, !alias.scope !65
  %187 = lshr i64 %184, 32
  %188 = add nuw nsw i64 %167, 1
  %189 = icmp eq i64 %188, %163
  br i1 %189, label %164, label %166
190:                                              ; preds = %164
  %191 = icmp eq i32 %159, 1400
  br i1 %191, label %192, label %193

192:                                              ; preds = %190
  tail call void @llvm.trap()
  unreachable

193:                                              ; preds = %190
  %194 = add nuw nsw i32 %160, 1
  store i32 %194, ptr %9, align 4, !tbaa !34, !alias.scope !65
  %195 = getelementptr inbounds [1400 x i32], ptr %161, i64 0, i64 %163
  store i32 1, ptr %195, align 4, !tbaa !24, !alias.scope !65
  br label %196

196:                                              ; preds = %193, %164
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %8) #10, !noalias !65
  br label %197

197:                                              ; preds = %147, %152, %196
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %7) #10, !noalias !65
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %6) #10, !noalias !65
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %10) #10
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %10, i8 0, i64 5604, i1 false), !alias.scope !71
  %198 = getelementptr inbounds i8, ptr %10, i64 4
  store i32 1, ptr %10, align 4, !tbaa !34, !alias.scope !71
  store i32 1, ptr %198, align 4, !tbaa !24, !alias.scope !71
  call fastcc void @power(ptr noundef %10, i32 noundef %20, i32 noundef %48) #11
  %199 = load i32, ptr %9, align 4, !tbaa !34
  %200 = load i32, ptr %10, align 4, !tbaa !34
  %201 = icmp eq i32 %199, %200
  br i1 %201, label %202, label %207

202:                                              ; preds = %197
  %203 = getelementptr inbounds i8, ptr %9, i64 4
  %204 = icmp eq i32 %199, 0
  br i1 %204, label %223, label %205

205:                                              ; preds = %202
  %206 = sext i32 %199 to i64
  br label %212

207:                                              ; preds = %197
  %208 = icmp slt i32 %199, %200
  %209 = select i1 %208, i32 -1, i32 1
  br label %223

210:                                              ; preds = %212
  %211 = icmp eq i64 %214, 0
  br i1 %211, label %223, label %212
212:                                              ; preds = %205, %210
  %213 = phi i64 [ %206, %205 ], [ %214, %210 ]
  %214 = add nsw i64 %213, -1
  %215 = getelementptr inbounds [1400 x i32], ptr %203, i64 0, i64 %214
  %216 = load i32, ptr %215, align 4, !tbaa !24
  %217 = getelementptr inbounds [1400 x i32], ptr %198, i64 0, i64 %214
  %218 = load i32, ptr %217, align 4, !tbaa !24
  %219 = icmp eq i32 %216, %218
  br i1 %219, label %210, label %220
220:                                              ; preds = %212
  %221 = icmp ult i32 %216, %218
  %222 = select i1 %221, i32 -1, i32 1
  br label %223

223:                                              ; preds = %210, %202, %207, %220
  %224 = phi i32 [ %209, %207 ], [ %222, %220 ], [ 0, %202 ], [ 0, %210 ]
  %225 = icmp sgt i32 %224, -1
  br i1 %225, label %226, label %285

226:                                              ; preds = %223
  %227 = icmp eq i32 %199, 0
  br i1 %227, label %232, label %228

228:                                              ; preds = %226
  %229 = getelementptr inbounds i8, ptr %9, i64 4
  %230 = zext i32 %20 to i64
  %231 = sext i32 %199 to i64
  br label %245

232:                                              ; preds = %245, %226
  %233 = getelementptr inbounds i8, ptr %9, i64 4
  br i1 %227, label %258, label %234

234:                                              ; preds = %232
  %235 = sext i32 %199 to i64
  br label %236

236:                                              ; preds = %242, %234
  %237 = phi i64 [ %235, %234 ], [ %238, %242 ]
  %238 = add nsw i64 %237, -1
  %239 = getelementptr inbounds [1400 x i32], ptr %233, i64 0, i64 %238
  %240 = load i32, ptr %239, align 4, !tbaa !24
  %241 = icmp eq i32 %240, 0
  br i1 %241, label %242, label %258

242:                                              ; preds = %236
  %243 = trunc i64 %238 to i32
  store i32 %243, ptr %9, align 4, !tbaa !34
  %244 = icmp eq i64 %238, 0
  br i1 %244, label %258, label %236
245:                                              ; preds = %245, %228
  %246 = phi i64 [ %231, %228 ], [ %248, %245 ]
  %247 = phi i64 [ 0, %228 ], [ %256, %245 ]
  %248 = add nsw i64 %246, -1
  %249 = shl nuw i64 %247, 32
  %250 = getelementptr inbounds [1400 x i32], ptr %229, i64 0, i64 %248
  %251 = load i32, ptr %250, align 4, !tbaa !24
  %252 = zext i32 %251 to i64
  %253 = or i64 %249, %252
  %254 = udiv i64 %253, %230
  %255 = trunc i64 %254 to i32
  store i32 %255, ptr %250, align 4, !tbaa !24
  %256 = urem i64 %253, %230
  %257 = icmp eq i64 %248, 0
  br i1 %257, label %232, label %245
258:                                              ; preds = %236, %242, %232
  %259 = add nsw i32 %54, 1
  %260 = icmp slt i32 %54, %56
  br i1 %260, label %285, label %261

261:                                              ; preds = %258
  %262 = icmp eq i32 %20, 2
  br i1 %262, label %263, label %277

263:                                              ; preds = %261
  %264 = getelementptr inbounds i8, ptr %4, i64 20
  %265 = load i32, ptr %264, align 4, !tbaa !16
  %266 = shl nsw i32 %265, 1
  %267 = or i32 %266, 1
  %268 = sext i32 %267 to i128
  %269 = getelementptr inbounds i8, ptr %4, i64 16
  %270 = load i32, ptr %269, align 8, !tbaa !15
  %271 = zext i32 %270 to i128
  %272 = shl i128 %268, %271
  %273 = or i128 %272, %17
  %274 = trunc i128 %273 to i64
  %275 = lshr i128 %273, 64
  %276 = trunc i128 %275 to i64
  br label %432

277:                                              ; preds = %261
  %278 = add nsw i32 %14, -6
  %279 = zext i32 %278 to i128
  %280 = shl i128 30, %279
  %281 = or i128 %17, %280
  %282 = trunc i128 %281 to i64
  %283 = lshr i128 %281, 64
  %284 = trunc i128 %283 to i64
  br label %432

285:                                              ; preds = %258, %223
  %286 = phi i32 [ %259, %258 ], [ %54, %223 ]
  %287 = icmp eq i32 %20, 10
  br i1 %287, label %288, label %336

288:                                              ; preds = %285
  %289 = getelementptr inbounds i8, ptr %11, i64 4
  br label %290

290:                                              ; preds = %288, %334
  %291 = phi i32 [ %335, %334 ], [ %286, %288 ]
  %292 = icmp slt i32 %291, %2
  br i1 %292, label %293, label %336

293:                                              ; preds = %290
  %294 = icmp slt i32 %291, %56
  %295 = load i32, ptr %9, align 4
  %296 = icmp ne i32 %295, 0
  %297 = select i1 %294, i1 %296, i1 false
  br i1 %297, label %298, label %336

298:                                              ; preds = %293
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %11) #10
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %11, ptr noundef nonnull align 4 dereferenceable(5604) %9, i64 5604, i1 false), !tbaa.struct !30
  %299 = load i32, ptr %11, align 4, !tbaa !34
  %300 = icmp eq i32 %299, 0
  br i1 %300, label %305, label %301

301:                                              ; preds = %298
  %302 = sext i32 %299 to i64
  br label %318

303:                                              ; preds = %318
  %304 = icmp eq i64 %329, 0
  br label %305

305:                                              ; preds = %303, %298
  %306 = phi i1 [ true, %298 ], [ %304, %303 ]
  br i1 %300, label %331, label %307

307:                                              ; preds = %305
  %308 = sext i32 %299 to i64
  br label %309

309:                                              ; preds = %315, %307
  %310 = phi i64 [ %308, %307 ], [ %311, %315 ]
  %311 = add nsw i64 %310, -1
  %312 = getelementptr inbounds [1400 x i32], ptr %289, i64 0, i64 %311
  %313 = load i32, ptr %312, align 4, !tbaa !24
  %314 = icmp eq i32 %313, 0
  br i1 %314, label %315, label %331

315:                                              ; preds = %309
  %316 = trunc i64 %311 to i32
  store i32 %316, ptr %11, align 4, !tbaa !34
  %317 = icmp eq i64 %311, 0
  br i1 %317, label %331, label %309
318:                                              ; preds = %318, %301
  %319 = phi i64 [ %302, %301 ], [ %321, %318 ]
  %320 = phi i64 [ 0, %301 ], [ %329, %318 ]
  %321 = add nsw i64 %319, -1
  %322 = shl nuw nsw i64 %320, 32
  %323 = getelementptr inbounds [1400 x i32], ptr %289, i64 0, i64 %321
  %324 = load i32, ptr %323, align 4, !tbaa !24
  %325 = zext i32 %324 to i64
  %326 = or i64 %322, %325
  %327 = udiv i64 %326, 10
  %328 = trunc i64 %327 to i32
  store i32 %328, ptr %323, align 4, !tbaa !24
  %329 = urem i64 %326, 10
  %330 = icmp eq i64 %321, 0
  br i1 %330, label %303, label %318
331:                                              ; preds = %309, %315, %305
  br i1 %306, label %332, label %334

332:                                              ; preds = %331
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %9, ptr noundef nonnull align 4 dereferenceable(5604) %11, i64 5604, i1 false), !tbaa.struct !30
  %333 = add nsw i32 %291, 1
  br label %334

334:                                              ; preds = %331, %332
  %335 = phi i32 [ %333, %332 ], [ %291, %331 ]
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %11) #10
  br i1 %306, label %290, label %336

336:                                              ; preds = %334, %290, %293, %285
  %337 = phi i32 [ %286, %285 ], [ %335, %334 ], [ %291, %293 ], [ %291, %290 ]
  %338 = load i32, ptr %9, align 4, !tbaa !34
  %339 = icmp sgt i32 %338, 4
  br i1 %339, label %345, label %340

340:                                              ; preds = %336
  %341 = icmp eq i32 %338, 0
  br i1 %341, label %360, label %342

342:                                              ; preds = %340
  %343 = getelementptr inbounds i8, ptr %9, i64 4
  %344 = sext i32 %338 to i64
  br label %350

345:                                              ; preds = %336
  tail call void @llvm.trap()
  unreachable

346:                                              ; preds = %350
  %347 = lshr i128 %354, 64
  %348 = trunc i128 %347 to i64
  %349 = trunc i128 %358 to i64
  br label %360

350:                                              ; preds = %350, %342
  %351 = phi i64 [ %344, %342 ], [ %353, %350 ]
  %352 = phi i128 [ 0, %342 ], [ %358, %350 ]
  %353 = add nsw i64 %351, -1
  %354 = shl i128 %352, 32
  %355 = getelementptr inbounds [1400 x i32], ptr %343, i64 0, i64 %353
  %356 = load i32, ptr %355, align 4, !tbaa !24
  %357 = zext i32 %356 to i128
  %358 = or i128 %354, %357
  %359 = icmp eq i64 %353, 0
  br i1 %359, label %346, label %350
360:                                              ; preds = %340, %346
  %361 = phi i64 [ 0, %340 ], [ %349, %346 ]
  %362 = phi i64 [ 0, %340 ], [ %348, %346 ]
  %363 = zext i64 %362 to i128
  %364 = shl nuw i128 %363, 64
  %365 = zext i64 %361 to i128
  %366 = or i128 %364, %365
  %367 = icmp eq i32 %20, 2
  br i1 %367, label %368, label %391

368:                                              ; preds = %360
  %369 = getelementptr inbounds i8, ptr %4, i64 16
  %370 = load i32, ptr %369, align 8, !tbaa !15
  %371 = zext i32 %370 to i128
  %372 = lshr i128 %366, %371
  %373 = icmp eq i128 %372, 0
  %374 = add nsw i32 %370, %337
  %375 = getelementptr inbounds i8, ptr %4, i64 20
  %376 = load i32, ptr %375, align 4
  %377 = add nsw i32 %374, %376
  %378 = sext i32 %377 to i128
  %379 = select i1 %373, i128 0, i128 %378
  %380 = shl i128 %379, %371
  %381 = icmp eq i32 %370, 128
  %382 = shl nsw i128 -1, %371
  %383 = xor i128 %382, -1
  %384 = select i1 %381, i128 -1, i128 %383
  %385 = and i128 %384, %366
  %386 = or i128 %385, %380
  %387 = or i128 %386, %17
  %388 = trunc i128 %387 to i64
  %389 = lshr i128 %387, 64
  %390 = trunc i128 %389 to i64
  br label %432

391:                                              ; preds = %360
  %392 = getelementptr inbounds i8, ptr %4, i64 20
  %393 = load i32, ptr %392, align 4, !tbaa !16
  %394 = add nsw i32 %393, %337
  %395 = icmp eq i32 %14, 128
  br i1 %395, label %421, label %396

396:                                              ; preds = %391
  %397 = getelementptr inbounds i8, ptr %4, i64 16
  %398 = load i32, ptr %397, align 8, !tbaa !15
  %399 = zext i32 %398 to i128
  %400 = lshr i128 %366, %399
  %401 = icmp eq i128 %400, 0
  br i1 %401, label %421, label %402

402:                                              ; preds = %396
  %403 = add nsw i32 %14, -3
  %404 = zext i32 %403 to i128
  %405 = shl i128 3, %404
  %406 = sext i32 %394 to i128
  %407 = add nsw i32 %398, -2
  %408 = zext i32 %407 to i128
  %409 = shl i128 %406, %408
  %410 = icmp eq i32 %407, 128
  %411 = shl nsw i128 -1, %408
  %412 = xor i128 %411, -1
  %413 = select i1 %410, i128 -1, i128 %412
  %414 = and i128 %413, %366
  %415 = or i128 %405, %409
  %416 = or i128 %415, %414
  %417 = or i128 %416, %17
  %418 = trunc i128 %417 to i64
  %419 = lshr i128 %417, 64
  %420 = trunc i128 %419 to i64
  br label %432

421:                                              ; preds = %396, %391
  %422 = sext i32 %394 to i128
  %423 = getelementptr inbounds i8, ptr %4, i64 16
  %424 = load i32, ptr %423, align 8, !tbaa !15
  %425 = zext i32 %424 to i128
  %426 = shl i128 %422, %425
  %427 = or i128 %17, %426
  %428 = or i128 %427, %366
  %429 = trunc i128 %428 to i64
  %430 = lshr i128 %428, 64
  %431 = trunc i128 %430 to i64
  br label %432

432:                                              ; preds = %277, %263, %368, %421, %402
  %433 = phi i64 [ %282, %277 ], [ %274, %263 ], [ %388, %368 ], [ %429, %421 ], [ %418, %402 ]
  %434 = phi i64 [ %284, %277 ], [ %276, %263 ], [ %390, %368 ], [ %431, %421 ], [ %420, %402 ]
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %10) #10
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %9) #10
  br label %435

435:                                              ; preds = %74, %60, %432, %39
  %436 = phi i64 [ %42, %39 ], [ %433, %432 ], [ %71, %60 ], [ %79, %74 ]
  %437 = phi i64 [ %44, %39 ], [ %434, %432 ], [ %73, %60 ], [ %81, %74 ]
  %438 = insertvalue { i64, i64 } poison, i64 %436, 0
  %439 = insertvalue { i64, i64 } %438, i64 %437, 1
  ret { i64, i64 } %439
}

; Function Attrs: nounwind memory(argmem: readwrite, inaccessiblemem: write)
define weak hidden i32 @tz_soft_cmp(ptr noundef readonly %0, ptr noundef readonly %1, i32 noundef %2) local_unnamed_addr #2 {
  %4 = alloca %struct.tzrt_format, align 8
  %5 = alloca %struct.tzrt_number, align 4
  %6 = alloca %struct.tzrt_number, align 4
  call void @llvm.lifetime.start.p0(i64 36, ptr nonnull %4) #10
  switch i32 %2, label %70 [
    i32 0, label %7
    i32 1, label %16
    i32 2, label %25
    i32 3, label %34
    i32 4, label %43
    i32 5, label %52
    i32 6, label %61
  ]

7:                                                ; preds = %3
  store i32 2, ptr %4, align 8, !tbaa !4, !alias.scope !76
  %8 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 11, ptr %8, align 4, !tbaa !12, !alias.scope !76
  %9 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 -24, ptr %9, align 8, !tbaa !13, !alias.scope !76
  %10 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 5, ptr %10, align 4, !tbaa !14, !alias.scope !76
  %11 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 10, ptr %11, align 8, !tbaa !15, !alias.scope !76
  %12 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 15, ptr %12, align 4, !tbaa !16, !alias.scope !76
  %13 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 16, ptr %13, align 8, !tbaa !17, !alias.scope !76
  %14 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 0, ptr %14, align 4, !tbaa !18, !alias.scope !76
  %15 = getelementptr inbounds i8, ptr %4, i64 32
  store i32 1, ptr %15, align 8, !tbaa !19, !alias.scope !76
  br label %83

16:                                               ; preds = %3
  store i32 2, ptr %4, align 8, !tbaa !4, !alias.scope !76
  %17 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 24, ptr %17, align 4, !tbaa !12, !alias.scope !76
  %18 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 -149, ptr %18, align 8, !tbaa !13, !alias.scope !76
  %19 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 104, ptr %19, align 4, !tbaa !14, !alias.scope !76
  %20 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 23, ptr %20, align 8, !tbaa !15, !alias.scope !76
  %21 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 127, ptr %21, align 4, !tbaa !16, !alias.scope !76
  %22 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 32, ptr %22, align 8, !tbaa !17, !alias.scope !76
  %23 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 0, ptr %23, align 4, !tbaa !18, !alias.scope !76
  %24 = getelementptr inbounds i8, ptr %4, i64 32
  store i32 1, ptr %24, align 8, !tbaa !19, !alias.scope !76
  br label %83

25:                                               ; preds = %3
  store i32 2, ptr %4, align 8, !tbaa !4, !alias.scope !76
  %26 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 53, ptr %26, align 4, !tbaa !12, !alias.scope !76
  %27 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 -1074, ptr %27, align 8, !tbaa !13, !alias.scope !76
  %28 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 971, ptr %28, align 4, !tbaa !14, !alias.scope !76
  %29 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 52, ptr %29, align 8, !tbaa !15, !alias.scope !76
  %30 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 1023, ptr %30, align 4, !tbaa !16, !alias.scope !76
  %31 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 64, ptr %31, align 8, !tbaa !17, !alias.scope !76
  %32 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 0, ptr %32, align 4, !tbaa !18, !alias.scope !76
  %33 = getelementptr inbounds i8, ptr %4, i64 32
  store i32 1, ptr %33, align 8, !tbaa !19, !alias.scope !76
  br label %83

34:                                               ; preds = %3
  store i32 2, ptr %4, align 8, !tbaa !4, !alias.scope !76
  %35 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 113, ptr %35, align 4, !tbaa !12, !alias.scope !76
  %36 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 -16494, ptr %36, align 8, !tbaa !13, !alias.scope !76
  %37 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 16271, ptr %37, align 4, !tbaa !14, !alias.scope !76
  %38 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 112, ptr %38, align 8, !tbaa !15, !alias.scope !76
  %39 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 16383, ptr %39, align 4, !tbaa !16, !alias.scope !76
  %40 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 128, ptr %40, align 8, !tbaa !17, !alias.scope !76
  %41 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 0, ptr %41, align 4, !tbaa !18, !alias.scope !76
  %42 = getelementptr inbounds i8, ptr %4, i64 32
  store i32 1, ptr %42, align 8, !tbaa !19, !alias.scope !76
  br label %83

43:                                               ; preds = %3
  store i32 10, ptr %4, align 8, !tbaa !4, !alias.scope !76
  %44 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 7, ptr %44, align 4, !tbaa !12, !alias.scope !76
  %45 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 -101, ptr %45, align 8, !tbaa !13, !alias.scope !76
  %46 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 90, ptr %46, align 4, !tbaa !14, !alias.scope !76
  %47 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 23, ptr %47, align 8, !tbaa !15, !alias.scope !76
  %48 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 101, ptr %48, align 4, !tbaa !16, !alias.scope !76
  %49 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 32, ptr %49, align 8, !tbaa !17, !alias.scope !76
  %50 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 0, ptr %50, align 4, !tbaa !18, !alias.scope !76
  %51 = getelementptr inbounds i8, ptr %4, i64 32
  store i32 1, ptr %51, align 8, !tbaa !19, !alias.scope !76
  br label %83

52:                                               ; preds = %3
  store i32 10, ptr %4, align 8, !tbaa !4, !alias.scope !76
  %53 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 16, ptr %53, align 4, !tbaa !12, !alias.scope !76
  %54 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 -398, ptr %54, align 8, !tbaa !13, !alias.scope !76
  %55 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 369, ptr %55, align 4, !tbaa !14, !alias.scope !76
  %56 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 53, ptr %56, align 8, !tbaa !15, !alias.scope !76
  %57 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 398, ptr %57, align 4, !tbaa !16, !alias.scope !76
  %58 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 64, ptr %58, align 8, !tbaa !17, !alias.scope !76
  %59 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 0, ptr %59, align 4, !tbaa !18, !alias.scope !76
  %60 = getelementptr inbounds i8, ptr %4, i64 32
  store i32 1, ptr %60, align 8, !tbaa !19, !alias.scope !76
  br label %83

61:                                               ; preds = %3
  store i32 10, ptr %4, align 8, !tbaa !4, !alias.scope !76
  %62 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 34, ptr %62, align 4, !tbaa !12, !alias.scope !76
  %63 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 -6176, ptr %63, align 8, !tbaa !13, !alias.scope !76
  %64 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 6111, ptr %64, align 4, !tbaa !14, !alias.scope !76
  %65 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 113, ptr %65, align 8, !tbaa !15, !alias.scope !76
  %66 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 6176, ptr %66, align 4, !tbaa !16, !alias.scope !76
  %67 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 128, ptr %67, align 8, !tbaa !17, !alias.scope !76
  %68 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 0, ptr %68, align 4, !tbaa !18, !alias.scope !76
  %69 = getelementptr inbounds i8, ptr %4, i64 32
  store i32 1, ptr %69, align 8, !tbaa !19, !alias.scope !76
  br label %83

70:                                               ; preds = %3
  %71 = and i32 %2, 7
  %72 = shl nuw nsw i32 8, %71
  store i32 2, ptr %4, align 8, !tbaa !4, !alias.scope !76
  %73 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 %72, ptr %73, align 4, !tbaa !12, !alias.scope !76
  %74 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 0, ptr %74, align 8, !tbaa !13, !alias.scope !76
  %75 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 0, ptr %75, align 4, !tbaa !14, !alias.scope !76
  %76 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 0, ptr %76, align 8, !tbaa !15, !alias.scope !76
  %77 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 0, ptr %77, align 4, !tbaa !16, !alias.scope !76
  %78 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 %72, ptr %78, align 8, !tbaa !17, !alias.scope !76
  %79 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 1, ptr %79, align 4, !tbaa !18, !alias.scope !76
  %80 = getelementptr inbounds i8, ptr %4, i64 32
  %81 = icmp slt i32 %2, 24
  %82 = zext i1 %81 to i32
  store i32 %82, ptr %80, align 8, !tbaa !19, !alias.scope !76
  br label %83

83:                                               ; preds = %7, %16, %25, %34, %43, %52, %61, %70
  %84 = phi i32 [ 2, %7 ], [ 2, %16 ], [ 2, %25 ], [ 2, %34 ], [ 10, %43 ], [ 10, %52 ], [ 10, %61 ], [ 2, %70 ]
  call void @llvm.lifetime.start.p0(i64 5616, ptr nonnull %5) #10
  call fastcc void @decode(ptr sret(%struct.tzrt_number) align 4 %5, ptr noundef %0, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %4) #11
  call void @llvm.lifetime.start.p0(i64 5616, ptr nonnull %6) #10
  call fastcc void @decode(ptr sret(%struct.tzrt_number) align 4 %6, ptr noundef %1, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %4) #11
  %85 = getelementptr inbounds i8, ptr %5, i64 5612
  %86 = load i32, ptr %85, align 4, !tbaa !23
  %87 = icmp eq i32 %86, 2
  %88 = getelementptr inbounds i8, ptr %6, i64 5612
  %89 = load i32, ptr %88, align 4
  %90 = icmp eq i32 %89, 2
  %91 = select i1 %87, i1 true, i1 %90
  br i1 %91, label %153, label %92

92:                                               ; preds = %83
  %93 = icmp ne i32 %86, 0
  %94 = icmp ne i32 %89, 0
  %95 = select i1 %93, i1 true, i1 %94
  %96 = load i32, ptr %5, align 4
  %97 = icmp ne i32 %96, 0
  %98 = select i1 %95, i1 true, i1 %97
  %99 = load i32, ptr %6, align 4
  %100 = icmp ne i32 %99, 0
  %101 = select i1 %98, i1 true, i1 %100
  br i1 %101, label %102, label %153

102:                                              ; preds = %92
  %103 = getelementptr inbounds i8, ptr %5, i64 5608
  %104 = load i32, ptr %103, align 4, !tbaa !20
  %105 = getelementptr inbounds i8, ptr %6, i64 5608
  %106 = load i32, ptr %105, align 4, !tbaa !20
  %107 = icmp eq i32 %104, %106
  br i1 %107, label %111, label %108

108:                                              ; preds = %102
  %109 = icmp eq i32 %104, 0
  %110 = select i1 %109, i32 1, i32 -1
  br label %153

111:                                              ; preds = %102
  br i1 %95, label %112, label %114

112:                                              ; preds = %111
  %113 = tail call i32 @llvm.scmp.i32.i32(i32 %86, i32 %89)
  br label %147

114:                                              ; preds = %111
  %115 = getelementptr inbounds i8, ptr %5, i64 5604
  %116 = load i32, ptr %115, align 4, !tbaa !35
  %117 = getelementptr inbounds i8, ptr %6, i64 5604
  %118 = load i32, ptr %117, align 4, !tbaa !35
  %119 = tail call i32 @llvm.smin.i32(i32 %116, i32 %118)
  %120 = sub nsw i32 %116, %119
  call fastcc void @power(ptr noundef %5, i32 noundef %84, i32 noundef %120) #11
  %121 = sub nsw i32 %118, %119
  call fastcc void @power(ptr noundef %6, i32 noundef %84, i32 noundef %121) #11
  %122 = load i32, ptr %5, align 4, !tbaa !34
  %123 = load i32, ptr %6, align 4, !tbaa !34
  %124 = icmp eq i32 %122, %123
  br i1 %124, label %125, label %131

125:                                              ; preds = %114
  %126 = getelementptr inbounds i8, ptr %5, i64 4
  %127 = getelementptr inbounds i8, ptr %6, i64 4
  %128 = icmp eq i32 %122, 0
  br i1 %128, label %147, label %129

129:                                              ; preds = %125
  %130 = sext i32 %122 to i64
  br label %136

131:                                              ; preds = %114
  %132 = icmp slt i32 %122, %123
  %133 = select i1 %132, i32 -1, i32 1
  br label %147

134:                                              ; preds = %136
  %135 = icmp eq i64 %138, 0
  br i1 %135, label %147, label %136
136:                                              ; preds = %129, %134
  %137 = phi i64 [ %130, %129 ], [ %138, %134 ]
  %138 = add nsw i64 %137, -1
  %139 = getelementptr inbounds [1400 x i32], ptr %126, i64 0, i64 %138
  %140 = load i32, ptr %139, align 4, !tbaa !24
  %141 = getelementptr inbounds [1400 x i32], ptr %127, i64 0, i64 %138
  %142 = load i32, ptr %141, align 4, !tbaa !24
  %143 = icmp eq i32 %140, %142
  br i1 %143, label %134, label %144
144:                                              ; preds = %136
  %145 = icmp ult i32 %140, %142
  %146 = select i1 %145, i32 -1, i32 1
  br label %147

147:                                              ; preds = %134, %125, %144, %131, %112
  %148 = phi i32 [ %113, %112 ], [ %133, %131 ], [ %146, %144 ], [ 0, %125 ], [ 0, %134 ]
  %149 = load i32, ptr %103, align 4, !tbaa !20
  %150 = icmp eq i32 %149, 0
  %151 = sub nsw i32 0, %148
  %152 = select i1 %150, i32 %148, i32 %151
  br label %153

153:                                              ; preds = %92, %83, %147, %108
  %154 = phi i32 [ %110, %108 ], [ %152, %147 ], [ 2, %83 ], [ 0, %92 ]
  call void @llvm.lifetime.end.p0(i64 5616, ptr nonnull %6) #10
  call void @llvm.lifetime.end.p0(i64 5616, ptr nonnull %5) #10
  call void @llvm.lifetime.end.p0(i64 36, ptr nonnull %4) #10
  ret i32 %154
}

; Function Attrs: nounwind memory(argmem: readwrite, inaccessiblemem: readwrite)
define weak hidden void @tz_soft_cast(ptr noundef writeonly %0, ptr noundef readonly %1, i32 noundef %2, i32 noundef %3) local_unnamed_addr #0 {
  %5 = alloca %struct.tzrt_format, align 8
  %6 = alloca %struct.tzrt_format, align 8
  %7 = alloca %struct.tzrt_number, align 4
  %8 = alloca %struct.tzrt_big, align 4
  %9 = alloca %struct.tzrt_big, align 4
  %10 = alloca %struct.tzrt_big, align 4
  %11 = alloca %struct.tzrt_big, align 4
  %12 = alloca %struct.tzrt_big, align 4
  %13 = alloca %struct.tzrt_big, align 4
  call void @llvm.lifetime.start.p0(i64 36, ptr nonnull %5) #10
  switch i32 %2, label %77 [
    i32 0, label %14
    i32 1, label %23
    i32 2, label %32
    i32 3, label %41
    i32 4, label %50
    i32 5, label %59
    i32 6, label %68
  ]

14:                                               ; preds = %4
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !79
  %15 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 11, ptr %15, align 4, !tbaa !12, !alias.scope !79
  %16 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -24, ptr %16, align 8, !tbaa !13, !alias.scope !79
  %17 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 5, ptr %17, align 4, !tbaa !14, !alias.scope !79
  %18 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 10, ptr %18, align 8, !tbaa !15, !alias.scope !79
  %19 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 15, ptr %19, align 4, !tbaa !16, !alias.scope !79
  %20 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 16, ptr %20, align 8, !tbaa !17, !alias.scope !79
  %21 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %21, align 4, !tbaa !18, !alias.scope !79
  %22 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %22, align 8, !tbaa !19, !alias.scope !79
  br label %90

23:                                               ; preds = %4
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !79
  %24 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 24, ptr %24, align 4, !tbaa !12, !alias.scope !79
  %25 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -149, ptr %25, align 8, !tbaa !13, !alias.scope !79
  %26 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 104, ptr %26, align 4, !tbaa !14, !alias.scope !79
  %27 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 23, ptr %27, align 8, !tbaa !15, !alias.scope !79
  %28 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 127, ptr %28, align 4, !tbaa !16, !alias.scope !79
  %29 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 32, ptr %29, align 8, !tbaa !17, !alias.scope !79
  %30 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %30, align 4, !tbaa !18, !alias.scope !79
  %31 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %31, align 8, !tbaa !19, !alias.scope !79
  br label %90

32:                                               ; preds = %4
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !79
  %33 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 53, ptr %33, align 4, !tbaa !12, !alias.scope !79
  %34 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -1074, ptr %34, align 8, !tbaa !13, !alias.scope !79
  %35 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 971, ptr %35, align 4, !tbaa !14, !alias.scope !79
  %36 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 52, ptr %36, align 8, !tbaa !15, !alias.scope !79
  %37 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 1023, ptr %37, align 4, !tbaa !16, !alias.scope !79
  %38 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 64, ptr %38, align 8, !tbaa !17, !alias.scope !79
  %39 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %39, align 4, !tbaa !18, !alias.scope !79
  %40 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %40, align 8, !tbaa !19, !alias.scope !79
  br label %90

41:                                               ; preds = %4
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !79
  %42 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 113, ptr %42, align 4, !tbaa !12, !alias.scope !79
  %43 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -16494, ptr %43, align 8, !tbaa !13, !alias.scope !79
  %44 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 16271, ptr %44, align 4, !tbaa !14, !alias.scope !79
  %45 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 112, ptr %45, align 8, !tbaa !15, !alias.scope !79
  %46 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 16383, ptr %46, align 4, !tbaa !16, !alias.scope !79
  %47 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 128, ptr %47, align 8, !tbaa !17, !alias.scope !79
  %48 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %48, align 4, !tbaa !18, !alias.scope !79
  %49 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %49, align 8, !tbaa !19, !alias.scope !79
  br label %90

50:                                               ; preds = %4
  store i32 10, ptr %5, align 8, !tbaa !4, !alias.scope !79
  %51 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 7, ptr %51, align 4, !tbaa !12, !alias.scope !79
  %52 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -101, ptr %52, align 8, !tbaa !13, !alias.scope !79
  %53 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 90, ptr %53, align 4, !tbaa !14, !alias.scope !79
  %54 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 23, ptr %54, align 8, !tbaa !15, !alias.scope !79
  %55 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 101, ptr %55, align 4, !tbaa !16, !alias.scope !79
  %56 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 32, ptr %56, align 8, !tbaa !17, !alias.scope !79
  %57 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %57, align 4, !tbaa !18, !alias.scope !79
  %58 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %58, align 8, !tbaa !19, !alias.scope !79
  br label %90

59:                                               ; preds = %4
  store i32 10, ptr %5, align 8, !tbaa !4, !alias.scope !79
  %60 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 16, ptr %60, align 4, !tbaa !12, !alias.scope !79
  %61 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -398, ptr %61, align 8, !tbaa !13, !alias.scope !79
  %62 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 369, ptr %62, align 4, !tbaa !14, !alias.scope !79
  %63 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 53, ptr %63, align 8, !tbaa !15, !alias.scope !79
  %64 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 398, ptr %64, align 4, !tbaa !16, !alias.scope !79
  %65 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 64, ptr %65, align 8, !tbaa !17, !alias.scope !79
  %66 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %66, align 4, !tbaa !18, !alias.scope !79
  %67 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %67, align 8, !tbaa !19, !alias.scope !79
  br label %90

68:                                               ; preds = %4
  store i32 10, ptr %5, align 8, !tbaa !4, !alias.scope !79
  %69 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 34, ptr %69, align 4, !tbaa !12, !alias.scope !79
  %70 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -6176, ptr %70, align 8, !tbaa !13, !alias.scope !79
  %71 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 6111, ptr %71, align 4, !tbaa !14, !alias.scope !79
  %72 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 113, ptr %72, align 8, !tbaa !15, !alias.scope !79
  %73 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 6176, ptr %73, align 4, !tbaa !16, !alias.scope !79
  %74 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 128, ptr %74, align 8, !tbaa !17, !alias.scope !79
  %75 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %75, align 4, !tbaa !18, !alias.scope !79
  %76 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %76, align 8, !tbaa !19, !alias.scope !79
  br label %90

77:                                               ; preds = %4
  %78 = and i32 %2, 7
  %79 = shl nuw nsw i32 8, %78
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !79
  %80 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 %79, ptr %80, align 4, !tbaa !12, !alias.scope !79
  %81 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 0, ptr %81, align 8, !tbaa !13, !alias.scope !79
  %82 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 0, ptr %82, align 4, !tbaa !14, !alias.scope !79
  %83 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 0, ptr %83, align 8, !tbaa !15, !alias.scope !79
  %84 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 0, ptr %84, align 4, !tbaa !16, !alias.scope !79
  %85 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 %79, ptr %85, align 8, !tbaa !17, !alias.scope !79
  %86 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 1, ptr %86, align 4, !tbaa !18, !alias.scope !79
  %87 = getelementptr inbounds i8, ptr %5, i64 32
  %88 = icmp slt i32 %2, 24
  %89 = zext i1 %88 to i32
  store i32 %89, ptr %87, align 8, !tbaa !19, !alias.scope !79
  br label %90

90:                                               ; preds = %14, %23, %32, %41, %50, %59, %68, %77
  %91 = phi i32 [ 2, %14 ], [ 2, %23 ], [ 2, %32 ], [ 2, %41 ], [ 10, %50 ], [ 10, %59 ], [ 10, %68 ], [ 2, %77 ]
  call void @llvm.lifetime.start.p0(i64 36, ptr nonnull %6) #10
  switch i32 %3, label %155 [
    i32 0, label %92
    i32 1, label %101
    i32 2, label %110
    i32 3, label %119
    i32 4, label %128
    i32 5, label %137
    i32 6, label %146
  ]

92:                                               ; preds = %90
  store i32 2, ptr %6, align 8, !tbaa !4, !alias.scope !82
  %93 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 11, ptr %93, align 4, !tbaa !12, !alias.scope !82
  %94 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 -24, ptr %94, align 8, !tbaa !13, !alias.scope !82
  %95 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 5, ptr %95, align 4, !tbaa !14, !alias.scope !82
  %96 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 10, ptr %96, align 8, !tbaa !15, !alias.scope !82
  %97 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 15, ptr %97, align 4, !tbaa !16, !alias.scope !82
  %98 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 16, ptr %98, align 8, !tbaa !17, !alias.scope !82
  %99 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 0, ptr %99, align 4, !tbaa !18, !alias.scope !82
  %100 = getelementptr inbounds i8, ptr %6, i64 32
  store i32 1, ptr %100, align 8, !tbaa !19, !alias.scope !82
  br label %168

101:                                              ; preds = %90
  store i32 2, ptr %6, align 8, !tbaa !4, !alias.scope !82
  %102 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 24, ptr %102, align 4, !tbaa !12, !alias.scope !82
  %103 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 -149, ptr %103, align 8, !tbaa !13, !alias.scope !82
  %104 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 104, ptr %104, align 4, !tbaa !14, !alias.scope !82
  %105 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 23, ptr %105, align 8, !tbaa !15, !alias.scope !82
  %106 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 127, ptr %106, align 4, !tbaa !16, !alias.scope !82
  %107 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 32, ptr %107, align 8, !tbaa !17, !alias.scope !82
  %108 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 0, ptr %108, align 4, !tbaa !18, !alias.scope !82
  %109 = getelementptr inbounds i8, ptr %6, i64 32
  store i32 1, ptr %109, align 8, !tbaa !19, !alias.scope !82
  br label %168

110:                                              ; preds = %90
  store i32 2, ptr %6, align 8, !tbaa !4, !alias.scope !82
  %111 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 53, ptr %111, align 4, !tbaa !12, !alias.scope !82
  %112 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 -1074, ptr %112, align 8, !tbaa !13, !alias.scope !82
  %113 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 971, ptr %113, align 4, !tbaa !14, !alias.scope !82
  %114 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 52, ptr %114, align 8, !tbaa !15, !alias.scope !82
  %115 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 1023, ptr %115, align 4, !tbaa !16, !alias.scope !82
  %116 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 64, ptr %116, align 8, !tbaa !17, !alias.scope !82
  %117 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 0, ptr %117, align 4, !tbaa !18, !alias.scope !82
  %118 = getelementptr inbounds i8, ptr %6, i64 32
  store i32 1, ptr %118, align 8, !tbaa !19, !alias.scope !82
  br label %168

119:                                              ; preds = %90
  store i32 2, ptr %6, align 8, !tbaa !4, !alias.scope !82
  %120 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 113, ptr %120, align 4, !tbaa !12, !alias.scope !82
  %121 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 -16494, ptr %121, align 8, !tbaa !13, !alias.scope !82
  %122 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 16271, ptr %122, align 4, !tbaa !14, !alias.scope !82
  %123 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 112, ptr %123, align 8, !tbaa !15, !alias.scope !82
  %124 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 16383, ptr %124, align 4, !tbaa !16, !alias.scope !82
  %125 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 128, ptr %125, align 8, !tbaa !17, !alias.scope !82
  %126 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 0, ptr %126, align 4, !tbaa !18, !alias.scope !82
  %127 = getelementptr inbounds i8, ptr %6, i64 32
  store i32 1, ptr %127, align 8, !tbaa !19, !alias.scope !82
  br label %168

128:                                              ; preds = %90
  store i32 10, ptr %6, align 8, !tbaa !4, !alias.scope !82
  %129 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 7, ptr %129, align 4, !tbaa !12, !alias.scope !82
  %130 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 -101, ptr %130, align 8, !tbaa !13, !alias.scope !82
  %131 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 90, ptr %131, align 4, !tbaa !14, !alias.scope !82
  %132 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 23, ptr %132, align 8, !tbaa !15, !alias.scope !82
  %133 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 101, ptr %133, align 4, !tbaa !16, !alias.scope !82
  %134 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 32, ptr %134, align 8, !tbaa !17, !alias.scope !82
  %135 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 0, ptr %135, align 4, !tbaa !18, !alias.scope !82
  %136 = getelementptr inbounds i8, ptr %6, i64 32
  store i32 1, ptr %136, align 8, !tbaa !19, !alias.scope !82
  br label %168

137:                                              ; preds = %90
  store i32 10, ptr %6, align 8, !tbaa !4, !alias.scope !82
  %138 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 16, ptr %138, align 4, !tbaa !12, !alias.scope !82
  %139 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 -398, ptr %139, align 8, !tbaa !13, !alias.scope !82
  %140 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 369, ptr %140, align 4, !tbaa !14, !alias.scope !82
  %141 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 53, ptr %141, align 8, !tbaa !15, !alias.scope !82
  %142 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 398, ptr %142, align 4, !tbaa !16, !alias.scope !82
  %143 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 64, ptr %143, align 8, !tbaa !17, !alias.scope !82
  %144 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 0, ptr %144, align 4, !tbaa !18, !alias.scope !82
  %145 = getelementptr inbounds i8, ptr %6, i64 32
  store i32 1, ptr %145, align 8, !tbaa !19, !alias.scope !82
  br label %168

146:                                              ; preds = %90
  store i32 10, ptr %6, align 8, !tbaa !4, !alias.scope !82
  %147 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 34, ptr %147, align 4, !tbaa !12, !alias.scope !82
  %148 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 -6176, ptr %148, align 8, !tbaa !13, !alias.scope !82
  %149 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 6111, ptr %149, align 4, !tbaa !14, !alias.scope !82
  %150 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 113, ptr %150, align 8, !tbaa !15, !alias.scope !82
  %151 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 6176, ptr %151, align 4, !tbaa !16, !alias.scope !82
  %152 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 128, ptr %152, align 8, !tbaa !17, !alias.scope !82
  %153 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 0, ptr %153, align 4, !tbaa !18, !alias.scope !82
  %154 = getelementptr inbounds i8, ptr %6, i64 32
  store i32 1, ptr %154, align 8, !tbaa !19, !alias.scope !82
  br label %168

155:                                              ; preds = %90
  %156 = and i32 %3, 7
  %157 = shl nuw nsw i32 8, %156
  store i32 2, ptr %6, align 8, !tbaa !4, !alias.scope !82
  %158 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 %157, ptr %158, align 4, !tbaa !12, !alias.scope !82
  %159 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 0, ptr %159, align 8, !tbaa !13, !alias.scope !82
  %160 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 0, ptr %160, align 4, !tbaa !14, !alias.scope !82
  %161 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 0, ptr %161, align 8, !tbaa !15, !alias.scope !82
  %162 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 0, ptr %162, align 4, !tbaa !16, !alias.scope !82
  %163 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 %157, ptr %163, align 8, !tbaa !17, !alias.scope !82
  %164 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 1, ptr %164, align 4, !tbaa !18, !alias.scope !82
  %165 = getelementptr inbounds i8, ptr %6, i64 32
  %166 = icmp slt i32 %3, 24
  %167 = zext i1 %166 to i32
  store i32 %167, ptr %165, align 8, !tbaa !19, !alias.scope !82
  br label %168

168:                                              ; preds = %92, %101, %110, %119, %128, %137, %146, %155
  %169 = phi i32 [ 1, %92 ], [ 1, %101 ], [ 1, %110 ], [ 1, %119 ], [ 1, %128 ], [ 1, %137 ], [ 1, %146 ], [ %167, %155 ]
  %170 = phi i1 [ true, %92 ], [ true, %101 ], [ true, %110 ], [ true, %119 ], [ true, %128 ], [ true, %137 ], [ true, %146 ], [ false, %155 ]
  %171 = phi i32 [ 16, %92 ], [ 32, %101 ], [ 64, %110 ], [ 128, %119 ], [ 32, %128 ], [ 64, %137 ], [ 128, %146 ], [ %157, %155 ]
  %172 = phi i32 [ 31, %92 ], [ 255, %101 ], [ 2047, %110 ], [ 32767, %119 ], [ 203, %128 ], [ 797, %137 ], [ 12353, %146 ], [ 1, %155 ]
  %173 = phi i32 [ 10, %92 ], [ 23, %101 ], [ 52, %110 ], [ 112, %119 ], [ 23, %128 ], [ 53, %137 ], [ 113, %146 ], [ 0, %155 ]
  %174 = phi i1 [ true, %92 ], [ true, %101 ], [ true, %110 ], [ true, %119 ], [ false, %128 ], [ false, %137 ], [ false, %146 ], [ true, %155 ]
  %175 = phi i32 [ 2, %92 ], [ 2, %101 ], [ 2, %110 ], [ 2, %119 ], [ 10, %128 ], [ 10, %137 ], [ 10, %146 ], [ 2, %155 ]
  call void @llvm.lifetime.start.p0(i64 5616, ptr nonnull %7) #10
  call fastcc void @decode(ptr sret(%struct.tzrt_number) align 4 %7, ptr noundef %1, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %5) #11
  br i1 %170, label %369, label %176

176:                                              ; preds = %168
  %177 = sub nuw nsw i32 %171, %169
  %178 = icmp eq i32 %177, 128
  %179 = zext i32 %177 to i128
  %180 = shl nsw i128 -1, %179
  %181 = xor i128 %180, -1
  %182 = select i1 %178, i128 -1, i128 %181
  %183 = icmp ne i32 %169, 0
  %184 = getelementptr inbounds i8, ptr %7, i64 5608
  %185 = load i32, ptr %184, align 4
  %186 = icmp ne i32 %185, 0
  %187 = select i1 %183, i1 %186, i1 false
  %188 = zext i1 %187 to i128
  %189 = add i128 %182, %188
  %190 = icmp eq i32 %169, 0
  %191 = select i1 %190, i1 %186, i1 false
  %192 = getelementptr inbounds i8, ptr %7, i64 5612
  %193 = load i32, ptr %192, align 4
  %194 = icmp eq i32 %193, 2
  %195 = or i1 %191, %194
  %196 = select i1 %195, i128 0, i128 %189
  %197 = icmp eq i32 %193, 0
  br i1 %197, label %212, label %198

198:                                              ; preds = %176
  %199 = icmp eq i32 %185, 0
  %200 = sub i128 0, %196
  %201 = select i1 %199, i128 %196, i128 %200
  %202 = lshr exact i32 %171, 3
  %203 = zext i32 %202 to i64
  br label %204

204:                                              ; preds = %204, %198
  %205 = phi i64 [ 0, %198 ], [ %210, %204 ]
  %206 = phi i128 [ %201, %198 ], [ %209, %204 ]
  %207 = trunc i128 %206 to i8
  %208 = getelementptr inbounds i8, ptr %0, i64 %205
  store i8 %207, ptr %208, align 1, !tbaa !26
  %209 = lshr i128 %206, 8
  %210 = add nuw nsw i64 %205, 1
  %211 = icmp eq i64 %210, %203
  br i1 %211, label %459, label %204
212:                                              ; preds = %176
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %8) #10
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %8, ptr noundef nonnull align 4 dereferenceable(5604) %7, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %9) #10
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %9, i8 0, i64 5604, i1 false), !alias.scope !85
  %213 = getelementptr inbounds i8, ptr %9, i64 4
  store i32 1, ptr %9, align 4, !tbaa !34, !alias.scope !85
  store i32 1, ptr %213, align 4, !tbaa !24, !alias.scope !85
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %10) #10
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %10, i8 0, i64 5604, i1 false), !alias.scope !88
  %214 = icmp eq i128 %196, 0
  br i1 %214, label %226, label %215

215:                                              ; preds = %212
  %216 = getelementptr inbounds i8, ptr %10, i64 4
  br label %217

217:                                              ; preds = %217, %215
  %218 = phi i128 [ %196, %215 ], [ %224, %217 ]
  %219 = trunc i128 %218 to i32
  %220 = load i32, ptr %10, align 4, !tbaa !34, !alias.scope !88
  %221 = add nsw i32 %220, 1
  store i32 %221, ptr %10, align 4, !tbaa !34, !alias.scope !88
  %222 = sext i32 %220 to i64
  %223 = getelementptr inbounds [1400 x i32], ptr %216, i64 0, i64 %222
  store i32 %219, ptr %223, align 4, !tbaa !24, !alias.scope !88
  %224 = lshr i128 %218, 32
  %225 = icmp ult i128 %218, 4294967296
  br i1 %225, label %226, label %217
226:                                              ; preds = %217, %212
  %227 = getelementptr inbounds i8, ptr %7, i64 5604
  %228 = load i32, ptr %227, align 4, !tbaa !35
  %229 = icmp sgt i32 %228, -1
  br i1 %229, label %230, label %231

230:                                              ; preds = %226
  call fastcc void @power(ptr noundef %8, i32 noundef %91, i32 noundef %228) #11
  br label %233

231:                                              ; preds = %226
  %232 = sub nsw i32 0, %228
  call fastcc void @power(ptr noundef %9, i32 noundef %91, i32 noundef %232) #11
  br label %233

233:                                              ; preds = %231, %230
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %11) #10
  tail call void @llvm.experimental.noalias.scope.decl(metadata !91)
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %11, i8 0, i64 5604, i1 false), !alias.scope !91
  %234 = load i32, ptr %10, align 4, !tbaa !34, !noalias !91
  %235 = load i32, ptr %9, align 4, !tbaa !34, !noalias !91
  %236 = add nsw i32 %235, %234
  %237 = icmp sgt i32 %236, 1400
  br i1 %237, label %238, label %239

238:                                              ; preds = %233
  tail call void @llvm.trap()
  unreachable

239:                                              ; preds = %233
  store i32 %236, ptr %11, align 4, !tbaa !34, !alias.scope !91
  %240 = icmp sgt i32 %234, 0
  br i1 %240, label %241, label %254

241:                                              ; preds = %239
  %242 = icmp sgt i32 %235, 0
  %243 = getelementptr inbounds i8, ptr %10, i64 4
  %244 = getelementptr inbounds i8, ptr %11, i64 4
  %245 = sext i32 %235 to i64
  %246 = zext i32 %234 to i64
  %247 = zext i32 %235 to i64
  br label %248

248:                                              ; preds = %271, %241
  %249 = phi i64 [ 0, %241 ], [ %275, %271 ]
  br i1 %242, label %250, label %271

250:                                              ; preds = %248
  %251 = getelementptr inbounds [1400 x i32], ptr %243, i64 0, i64 %249
  %252 = load i32, ptr %251, align 4, !tbaa !24, !noalias !91
  %253 = zext i32 %252 to i64
  br label %277

254:                                              ; preds = %271, %239
  %255 = getelementptr inbounds i8, ptr %11, i64 4
  %256 = load i32, ptr %11, align 4, !tbaa !34, !alias.scope !91
  %257 = icmp eq i32 %256, 0
  br i1 %257, label %294, label %258

258:                                              ; preds = %254
  %259 = sext i32 %256 to i64
  br label %260

260:                                              ; preds = %266, %258
  %261 = phi i64 [ %259, %258 ], [ %262, %266 ]
  %262 = add nsw i64 %261, -1
  %263 = getelementptr inbounds [1400 x i32], ptr %255, i64 0, i64 %262
  %264 = load i32, ptr %263, align 4, !tbaa !24, !alias.scope !91
  %265 = icmp eq i32 %264, 0
  br i1 %265, label %266, label %294

266:                                              ; preds = %260
  %267 = trunc i64 %262 to i32
  store i32 %267, ptr %11, align 4, !tbaa !34, !alias.scope !91
  %268 = icmp eq i64 %262, 0
  br i1 %268, label %294, label %260
269:                                              ; preds = %277
  %270 = trunc i64 %291 to i32
  br label %271

271:                                              ; preds = %269, %248
  %272 = phi i32 [ 0, %248 ], [ %270, %269 ]
  %273 = add nsw i64 %249, %245
  %274 = getelementptr inbounds [1400 x i32], ptr %244, i64 0, i64 %273
  store i32 %272, ptr %274, align 4, !tbaa !24, !alias.scope !91
  %275 = add nuw nsw i64 %249, 1
  %276 = icmp eq i64 %275, %246
  br i1 %276, label %254, label %248
277:                                              ; preds = %277, %250
  %278 = phi i64 [ 0, %250 ], [ %292, %277 ]
  %279 = phi i64 [ 0, %250 ], [ %291, %277 ]
  %280 = getelementptr inbounds [1400 x i32], ptr %213, i64 0, i64 %278
  %281 = load i32, ptr %280, align 4, !tbaa !24, !noalias !91
  %282 = zext i32 %281 to i64
  %283 = mul nuw i64 %282, %253
  %284 = add nuw nsw i64 %278, %249
  %285 = getelementptr inbounds [1400 x i32], ptr %244, i64 0, i64 %284
  %286 = load i32, ptr %285, align 4, !tbaa !24, !alias.scope !91
  %287 = zext i32 %286 to i64
  %288 = add nuw nsw i64 %279, %287
  %289 = add nuw i64 %288, %283
  %290 = trunc i64 %289 to i32
  store i32 %290, ptr %285, align 4, !tbaa !24, !alias.scope !91
  %291 = lshr i64 %289, 32
  %292 = add nuw nsw i64 %278, 1
  %293 = icmp eq i64 %292, %247
  br i1 %293, label %269, label %277
294:                                              ; preds = %260, %266, %254
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %10, ptr noundef nonnull align 4 dereferenceable(5604) %11, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %11) #10
  %295 = load i32, ptr %8, align 4, !tbaa !34
  %296 = load i32, ptr %10, align 4, !tbaa !34
  %297 = icmp eq i32 %295, %296
  br i1 %297, label %298, label %304

298:                                              ; preds = %294
  %299 = getelementptr inbounds i8, ptr %8, i64 4
  %300 = getelementptr inbounds i8, ptr %10, i64 4
  %301 = icmp eq i32 %295, 0
  br i1 %301, label %320, label %302

302:                                              ; preds = %298
  %303 = sext i32 %295 to i64
  br label %309

304:                                              ; preds = %294
  %305 = icmp slt i32 %295, %296
  %306 = select i1 %305, i32 -1, i32 1
  br label %320

307:                                              ; preds = %309
  %308 = icmp eq i64 %311, 0
  br i1 %308, label %320, label %309
309:                                              ; preds = %302, %307
  %310 = phi i64 [ %303, %302 ], [ %311, %307 ]
  %311 = add nsw i64 %310, -1
  %312 = getelementptr inbounds [1400 x i32], ptr %299, i64 0, i64 %311
  %313 = load i32, ptr %312, align 4, !tbaa !24
  %314 = getelementptr inbounds [1400 x i32], ptr %300, i64 0, i64 %311
  %315 = load i32, ptr %314, align 4, !tbaa !24
  %316 = icmp eq i32 %313, %315
  br i1 %316, label %307, label %317
317:                                              ; preds = %309
  %318 = icmp ult i32 %313, %315
  %319 = select i1 %318, i32 -1, i32 1
  br label %320

320:                                              ; preds = %307, %298, %304, %317
  %321 = phi i32 [ %306, %304 ], [ %319, %317 ], [ 0, %298 ], [ 0, %307 ]
  %322 = icmp sgt i32 %321, -1
  br i1 %322, label %353, label %323

323:                                              ; preds = %320
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %12) #10
  call fastcc void @divide(ptr sret(%struct.tzrt_big) align 4 %12, ptr noundef %8, ptr noundef %9) #11
  %324 = load i32, ptr %12, align 4, !tbaa !34
  %325 = icmp sgt i32 %324, 4
  br i1 %325, label %331, label %326

326:                                              ; preds = %323
  %327 = icmp eq i32 %324, 0
  br i1 %327, label %346, label %328

328:                                              ; preds = %326
  %329 = getelementptr inbounds i8, ptr %12, i64 4
  %330 = sext i32 %324 to i64
  br label %336

331:                                              ; preds = %323
  tail call void @llvm.trap()
  unreachable

332:                                              ; preds = %336
  %333 = lshr i128 %340, 64
  %334 = trunc i128 %333 to i64
  %335 = trunc i128 %344 to i64
  br label %346

336:                                              ; preds = %336, %328
  %337 = phi i64 [ %330, %328 ], [ %339, %336 ]
  %338 = phi i128 [ 0, %328 ], [ %344, %336 ]
  %339 = add nsw i64 %337, -1
  %340 = shl i128 %338, 32
  %341 = getelementptr inbounds [1400 x i32], ptr %329, i64 0, i64 %339
  %342 = load i32, ptr %341, align 4, !tbaa !24
  %343 = zext i32 %342 to i128
  %344 = or i128 %340, %343
  %345 = icmp eq i64 %339, 0
  br i1 %345, label %332, label %336
346:                                              ; preds = %326, %332
  %347 = phi i64 [ 0, %326 ], [ %335, %332 ]
  %348 = phi i64 [ 0, %326 ], [ %334, %332 ]
  %349 = zext i64 %348 to i128
  %350 = shl nuw i128 %349, 64
  %351 = zext i64 %347 to i128
  %352 = or i128 %350, %351
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %12) #10
  br label %353

353:                                              ; preds = %346, %320
  %354 = phi i128 [ %352, %346 ], [ %196, %320 ]
  %355 = icmp eq i32 %185, 0
  %356 = sub i128 0, %354
  %357 = select i1 %355, i128 %354, i128 %356
  %358 = lshr exact i32 %171, 3
  %359 = zext i32 %358 to i64
  br label %360

360:                                              ; preds = %360, %353
  %361 = phi i64 [ 0, %353 ], [ %366, %360 ]
  %362 = phi i128 [ %357, %353 ], [ %365, %360 ]
  %363 = trunc i128 %362 to i8
  %364 = getelementptr inbounds i8, ptr %0, i64 %361
  store i8 %363, ptr %364, align 1, !tbaa !26
  %365 = lshr i128 %362, 8
  %366 = add nuw nsw i64 %361, 1
  %367 = icmp eq i64 %366, %359
  br i1 %367, label %368, label %360
368:                                              ; preds = %360
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %10) #10
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %9) #10
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %8) #10
  br label %459

369:                                              ; preds = %168
  %370 = getelementptr inbounds i8, ptr %7, i64 5612
  %371 = load i32, ptr %370, align 4, !tbaa !23
  %372 = icmp eq i32 %371, 0
  br i1 %372, label %425, label %373

373:                                              ; preds = %369
  %374 = getelementptr inbounds i8, ptr %7, i64 5608
  %375 = load i32, ptr %374, align 4, !tbaa !20
  %376 = sext i32 %375 to i128
  %377 = add nsw i32 %171, -1
  %378 = zext i32 %377 to i128
  %379 = shl i128 %376, %378
  br i1 %174, label %380, label %397

380:                                              ; preds = %373
  %381 = zext i32 %172 to i128
  %382 = zext i32 %173 to i128
  %383 = shl nuw i128 %381, %382
  %384 = or i128 %379, %383
  %385 = icmp eq i32 %371, 2
  %386 = trunc i128 %384 to i64
  %387 = lshr i128 %384, 64
  %388 = trunc i128 %387 to i64
  br i1 %385, label %389, label %408

389:                                              ; preds = %380
  %390 = add nsw i32 %173, -1
  %391 = zext i32 %390 to i128
  %392 = shl nuw i128 1, %391
  %393 = or i128 %384, %392
  %394 = trunc i128 %393 to i64
  %395 = lshr i128 %393, 64
  %396 = trunc i128 %395 to i64
  br label %408

397:                                              ; preds = %373
  %398 = icmp eq i32 %371, 2
  %399 = select i1 %398, i32 31, i32 30
  %400 = zext i32 %399 to i128
  %401 = add nsw i32 %171, -6
  %402 = zext i32 %401 to i128
  %403 = shl i128 %400, %402
  %404 = or i128 %379, %403
  %405 = trunc i128 %404 to i64
  %406 = lshr i128 %404, 64
  %407 = trunc i128 %406 to i64
  br label %408

408:                                              ; preds = %397, %389, %380
  %409 = phi i64 [ %394, %389 ], [ %386, %380 ], [ %405, %397 ]
  %410 = phi i64 [ %396, %389 ], [ %388, %380 ], [ %407, %397 ]
  %411 = lshr exact i32 %171, 3
  %412 = zext i64 %410 to i128
  %413 = shl nuw i128 %412, 64
  %414 = zext i64 %409 to i128
  %415 = or i128 %413, %414
  %416 = zext i32 %411 to i64
  br label %417

417:                                              ; preds = %417, %408
  %418 = phi i64 [ 0, %408 ], [ %423, %417 ]
  %419 = phi i128 [ %415, %408 ], [ %422, %417 ]
  %420 = trunc i128 %419 to i8
  %421 = getelementptr inbounds i8, ptr %0, i64 %418
  store i8 %420, ptr %421, align 1, !tbaa !26
  %422 = lshr i128 %419, 8
  %423 = add nuw nsw i64 %418, 1
  %424 = icmp eq i64 %423, %416
  br i1 %424, label %459, label %417
425:                                              ; preds = %369
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %13) #10
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %13, i8 0, i64 5604, i1 false), !alias.scope !94
  %426 = getelementptr inbounds i8, ptr %13, i64 4
  store i32 1, ptr %13, align 4, !tbaa !34, !alias.scope !94
  store i32 1, ptr %426, align 4, !tbaa !24, !alias.scope !94
  %427 = icmp eq i32 %91, %175
  br i1 %427, label %436, label %428

428:                                              ; preds = %425
  %429 = getelementptr inbounds i8, ptr %7, i64 5604
  %430 = load i32, ptr %429, align 4, !tbaa !35
  %431 = icmp sgt i32 %430, -1
  br i1 %431, label %432, label %433

432:                                              ; preds = %428
  call fastcc void @power(ptr noundef %7, i32 noundef %91, i32 noundef %430) #11
  br label %435

433:                                              ; preds = %428
  %434 = sub nsw i32 0, %430
  call fastcc void @power(ptr noundef %13, i32 noundef %91, i32 noundef %434) #11
  br label %435

435:                                              ; preds = %433, %432
  store i32 0, ptr %429, align 4, !tbaa !35
  br label %436

436:                                              ; preds = %435, %425
  %437 = getelementptr inbounds i8, ptr %7, i64 5604
  %438 = load i32, ptr %437, align 4, !tbaa !35
  %439 = getelementptr inbounds i8, ptr %7, i64 5608
  %440 = load i32, ptr %439, align 4, !tbaa !20
  %441 = call fastcc { i64, i64 } @pack(ptr noundef %7, ptr noundef %13, i32 noundef %438, i32 noundef %440, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %6) #11
  %442 = extractvalue { i64, i64 } %441, 1
  %443 = extractvalue { i64, i64 } %441, 0
  %444 = lshr exact i32 %171, 3
  %445 = zext i64 %442 to i128
  %446 = shl nuw i128 %445, 64
  %447 = zext i64 %443 to i128
  %448 = or i128 %446, %447
  %449 = zext i32 %444 to i64
  br label %450

450:                                              ; preds = %450, %436
  %451 = phi i64 [ 0, %436 ], [ %456, %450 ]
  %452 = phi i128 [ %448, %436 ], [ %455, %450 ]
  %453 = trunc i128 %452 to i8
  %454 = getelementptr inbounds i8, ptr %0, i64 %451
  store i8 %453, ptr %454, align 1, !tbaa !26
  %455 = lshr i128 %452, 8
  %456 = add nuw nsw i64 %451, 1
  %457 = icmp eq i64 %456, %449
  br i1 %457, label %458, label %450
458:                                              ; preds = %450
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %13) #10
  br label %459

459:                                              ; preds = %204, %417, %368, %458
  call void @llvm.lifetime.end.p0(i64 5616, ptr nonnull %7) #10
  call void @llvm.lifetime.end.p0(i64 36, ptr nonnull %6) #10
  call void @llvm.lifetime.end.p0(i64 36, ptr nonnull %5) #10
  ret void
}

; Function Attrs: nounwind memory(argmem: readwrite, inaccessiblemem: write)
define internal fastcc void @divide(ptr noalias nonnull sret(%struct.tzrt_big) align 4 %0, ptr noundef nonnull %1, ptr noundef nonnull readonly %2) unnamed_addr #2 {
  %4 = alloca %struct.tzrt_big, align 4
  tail call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %0, i8 0, i64 5604, i1 false)
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %4) #10
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %4, ptr noundef nonnull align 4 dereferenceable(5604) %2, i64 5604, i1 false), !tbaa.struct !30
  %5 = load i32, ptr %2, align 4, !tbaa !34
  %6 = icmp eq i32 %5, 0
  br i1 %6, label %7, label %8

7:                                                ; preds = %3
  tail call void @llvm.trap()
  unreachable

8:                                                ; preds = %3
  %9 = load i32, ptr %1, align 4, !tbaa !34
  %10 = icmp eq i32 %9, 0
  br i1 %10, label %21, label %11

11:                                               ; preds = %8
  %12 = add nsw i32 %9, -1
  %13 = shl nsw i32 %12, 5
  %14 = add nsw i32 %13, 32
  %15 = getelementptr inbounds i8, ptr %1, i64 4
  %16 = sext i32 %12 to i64
  %17 = getelementptr inbounds [1400 x i32], ptr %15, i64 0, i64 %16
  %18 = load i32, ptr %17, align 4, !tbaa !24
  %19 = tail call i32 @llvm.ctlz.i32(i32 %18, i1 true)
  %20 = sub nsw i32 %14, %19
  br label %21

21:                                               ; preds = %11, %8
  %22 = phi i32 [ %20, %11 ], [ 0, %8 ]
  %23 = add nsw i32 %5, -1
  %24 = getelementptr inbounds i8, ptr %2, i64 4
  %25 = sext i32 %23 to i64
  %26 = getelementptr inbounds [1400 x i32], ptr %24, i64 0, i64 %25
  %27 = load i32, ptr %26, align 4, !tbaa !24
  %28 = tail call i32 @llvm.ctlz.i32(i32 %27, i1 true)
  %29 = shl i32 %23, 5
  %30 = sub i32 %22, %29
  %31 = add i32 %30, -32
  %32 = add i32 %31, %28
  %33 = icmp slt i32 %32, 0
  br i1 %33, label %213, label %34

34:                                               ; preds = %21
  %35 = load i32, ptr %4, align 4, !tbaa !34
  %36 = icmp ne i32 %35, 0
  %37 = icmp ne i32 %32, 0
  %38 = and i1 %37, %36
  br i1 %38, label %39, label %91

39:                                               ; preds = %34
  %40 = lshr i32 %32, 5
  %41 = and i32 %32, 31
  %42 = add nsw i32 %35, %40
  %43 = icmp sgt i32 %42, 1399
  br i1 %43, label %48, label %44

44:                                               ; preds = %39
  %45 = getelementptr inbounds i8, ptr %4, i64 4
  %46 = sext i32 %35 to i64
  %47 = zext i32 %40 to i64
  br label %51

48:                                               ; preds = %39
  tail call void @llvm.trap()
  unreachable

49:                                               ; preds = %51
  %50 = icmp sgt i32 %32, 31
  br i1 %50, label %66, label %59

51:                                               ; preds = %51, %44
  %52 = phi i64 [ %46, %44 ], [ %53, %51 ]
  %53 = add nsw i64 %52, -1
  %54 = getelementptr inbounds [1400 x i32], ptr %45, i64 0, i64 %53
  %55 = load i32, ptr %54, align 4, !tbaa !24
  %56 = add nsw i64 %53, %47
  %57 = getelementptr inbounds [1400 x i32], ptr %45, i64 0, i64 %56
  store i32 %55, ptr %57, align 4, !tbaa !24
  %58 = icmp eq i64 %53, 0
  br i1 %58, label %49, label %51
59:                                               ; preds = %66, %49
  %60 = load i32, ptr %4, align 4, !tbaa !34
  %61 = add nsw i32 %60, %40
  store i32 %61, ptr %4, align 4, !tbaa !34
  %62 = icmp sgt i32 %60, 0
  br i1 %62, label %63, label %71

63:                                               ; preds = %59
  %64 = zext i32 %41 to i64
  %65 = zext i32 %61 to i64
  br label %74

66:                                               ; preds = %49, %66
  %67 = phi i64 [ %69, %66 ], [ 0, %49 ]
  %68 = getelementptr inbounds [1400 x i32], ptr %45, i64 0, i64 %67
  store i32 0, ptr %68, align 4, !tbaa !24
  %69 = add nuw nsw i64 %67, 1
  %70 = icmp eq i64 %69, %47
  br i1 %70, label %59, label %66
71:                                               ; preds = %74, %59
  %72 = phi i64 [ 0, %59 ], [ %83, %74 ]
  %73 = icmp eq i64 %72, 0
  br i1 %73, label %91, label %86

74:                                               ; preds = %74, %63
  %75 = phi i64 [ %47, %63 ], [ %84, %74 ]
  %76 = phi i64 [ 0, %63 ], [ %83, %74 ]
  %77 = getelementptr inbounds [1400 x i32], ptr %45, i64 0, i64 %75
  %78 = load i32, ptr %77, align 4, !tbaa !24
  %79 = zext i32 %78 to i64
  %80 = shl nuw nsw i64 %79, %64
  %81 = or i64 %80, %76
  %82 = trunc i64 %81 to i32
  store i32 %82, ptr %77, align 4, !tbaa !24
  %83 = lshr i64 %80, 32
  %84 = add nuw nsw i64 %75, 1
  %85 = icmp samesign ult i64 %84, %65
  br i1 %85, label %74, label %71
86:                                               ; preds = %71
  %87 = trunc i64 %72 to i32
  %88 = add nsw i32 %61, 1
  store i32 %88, ptr %4, align 4, !tbaa !34
  %89 = sext i32 %61 to i64
  %90 = getelementptr inbounds [1400 x i32], ptr %45, i64 0, i64 %89
  store i32 %87, ptr %90, align 4, !tbaa !24
  br label %91

91:                                               ; preds = %34, %71, %86
  %92 = lshr i32 %32, 5
  %93 = add nuw nsw i32 %92, 1
  store i32 %93, ptr %0, align 4, !tbaa !34
  %94 = getelementptr inbounds i8, ptr %1, i64 4
  %95 = getelementptr inbounds i8, ptr %4, i64 4
  %96 = getelementptr inbounds i8, ptr %0, i64 4
  %97 = load i32, ptr %1, align 4, !tbaa !34
  br label %109

98:                                               ; preds = %210
  %99 = zext i32 %93 to i64
  br label %100

100:                                              ; preds = %106, %98
  %101 = phi i64 [ %99, %98 ], [ %102, %106 ]
  %102 = add nsw i64 %101, -1
  %103 = getelementptr inbounds [1400 x i32], ptr %96, i64 0, i64 %102
  %104 = load i32, ptr %103, align 4, !tbaa !24
  %105 = icmp eq i32 %104, 0
  br i1 %105, label %106, label %213

106:                                              ; preds = %100
  %107 = trunc i64 %102 to i32
  store i32 %107, ptr %0, align 4, !tbaa !34
  %108 = icmp eq i64 %102, 0
  br i1 %108, label %213, label %100
109:                                              ; preds = %91, %210
  %110 = phi i32 [ %97, %91 ], [ %187, %210 ]
  %111 = phi i32 [ %32, %91 ], [ %211, %210 ]
  %112 = load i32, ptr %4, align 4, !tbaa !34
  %113 = icmp eq i32 %110, %112
  br i1 %113, label %114, label %118

114:                                              ; preds = %109
  %115 = icmp eq i32 %110, 0
  br i1 %115, label %134, label %116

116:                                              ; preds = %114
  %117 = sext i32 %110 to i64
  br label %123

118:                                              ; preds = %109
  %119 = icmp slt i32 %110, %112
  %120 = select i1 %119, i32 -1, i32 1
  br label %134

121:                                              ; preds = %123
  %122 = icmp eq i64 %125, 0
  br i1 %122, label %134, label %123
123:                                              ; preds = %116, %121
  %124 = phi i64 [ %117, %116 ], [ %125, %121 ]
  %125 = add nsw i64 %124, -1
  %126 = getelementptr inbounds [1400 x i32], ptr %94, i64 0, i64 %125
  %127 = load i32, ptr %126, align 4, !tbaa !24
  %128 = getelementptr inbounds [1400 x i32], ptr %95, i64 0, i64 %125
  %129 = load i32, ptr %128, align 4, !tbaa !24
  %130 = icmp eq i32 %127, %129
  br i1 %130, label %121, label %131
131:                                              ; preds = %123
  %132 = icmp ult i32 %127, %129
  %133 = select i1 %132, i32 -1, i32 1
  br label %134

134:                                              ; preds = %121, %114, %118, %131
  %135 = phi i32 [ %120, %118 ], [ %133, %131 ], [ 0, %114 ], [ 0, %121 ]
  %136 = icmp sgt i32 %135, -1
  br i1 %136, label %137, label %186

137:                                              ; preds = %134
  %138 = icmp sgt i32 %110, 0
  br i1 %138, label %139, label %142

139:                                              ; preds = %137
  %140 = zext i32 %110 to i64
  %141 = sext i32 %112 to i64
  br label %155

142:                                              ; preds = %163, %137
  %143 = icmp eq i32 %110, 0
  br i1 %143, label %177, label %144

144:                                              ; preds = %142
  %145 = sext i32 %110 to i64
  br label %146

146:                                              ; preds = %152, %144
  %147 = phi i64 [ %145, %144 ], [ %148, %152 ]
  %148 = add nsw i64 %147, -1
  %149 = getelementptr inbounds [1400 x i32], ptr %94, i64 0, i64 %148
  %150 = load i32, ptr %149, align 4, !tbaa !24
  %151 = icmp eq i32 %150, 0
  br i1 %151, label %152, label %175

152:                                              ; preds = %146
  %153 = trunc i64 %148 to i32
  store i32 %153, ptr %1, align 4, !tbaa !34
  %154 = icmp eq i64 %148, 0
  br i1 %154, label %177, label %146
155:                                              ; preds = %163, %139
  %156 = phi i64 [ 0, %139 ], [ %173, %163 ]
  %157 = phi i64 [ 0, %139 ], [ %172, %163 ]
  %158 = icmp slt i64 %156, %141
  br i1 %158, label %159, label %163

159:                                              ; preds = %155
  %160 = getelementptr inbounds [1400 x i32], ptr %95, i64 0, i64 %156
  %161 = load i32, ptr %160, align 4, !tbaa !24
  %162 = zext i32 %161 to i64
  br label %163

163:                                              ; preds = %159, %155
  %164 = phi i64 [ %162, %159 ], [ 0, %155 ]
  %165 = add nuw nsw i64 %164, %157
  %166 = getelementptr inbounds [1400 x i32], ptr %94, i64 0, i64 %156
  %167 = load i32, ptr %166, align 4, !tbaa !24
  %168 = zext i32 %167 to i64
  %169 = trunc i64 %165 to i32
  %170 = sub i32 %167, %169
  store i32 %170, ptr %166, align 4, !tbaa !24
  %171 = icmp samesign ugt i64 %165, %168
  %172 = zext i1 %171 to i64
  %173 = add nuw nsw i64 %156, 1
  %174 = icmp eq i64 %173, %140
  br i1 %174, label %142, label %155
175:                                              ; preds = %146
  %176 = trunc i64 %147 to i32
  br label %177

177:                                              ; preds = %175, %152, %142
  %178 = phi i32 [ %110, %142 ], [ %176, %175 ], [ 0, %152 ]
  %179 = and i32 %111, 31
  %180 = shl nuw i32 1, %179
  %181 = lshr i32 %111, 5
  %182 = zext i32 %181 to i64
  %183 = getelementptr inbounds [1400 x i32], ptr %96, i64 0, i64 %182
  %184 = load i32, ptr %183, align 4, !tbaa !24
  %185 = or i32 %184, %180
  store i32 %185, ptr %183, align 4, !tbaa !24
  br label %186

186:                                              ; preds = %177, %134
  %187 = phi i32 [ %178, %177 ], [ %110, %134 ]
  %188 = icmp eq i32 %112, 0
  br i1 %188, label %210, label %189

189:                                              ; preds = %186
  %190 = sext i32 %112 to i64
  br label %200

191:                                              ; preds = %200, %197
  %192 = phi i64 [ %193, %197 ], [ %190, %200 ]
  %193 = add nsw i64 %192, -1
  %194 = getelementptr inbounds [1400 x i32], ptr %95, i64 0, i64 %193
  %195 = load i32, ptr %194, align 4, !tbaa !24
  %196 = icmp eq i32 %195, 0
  br i1 %196, label %197, label %210

197:                                              ; preds = %191
  %198 = trunc i64 %193 to i32
  store i32 %198, ptr %4, align 4, !tbaa !34
  %199 = icmp eq i64 %193, 0
  br i1 %199, label %210, label %191
200:                                              ; preds = %200, %189
  %201 = phi i64 [ %190, %189 ], [ %203, %200 ]
  %202 = phi i32 [ 0, %189 ], [ %208, %200 ]
  %203 = add nsw i64 %201, -1
  %204 = getelementptr inbounds [1400 x i32], ptr %95, i64 0, i64 %203
  %205 = load i32, ptr %204, align 4, !tbaa !24
  %206 = lshr i32 %205, 1
  %207 = or i32 %206, %202
  store i32 %207, ptr %204, align 4, !tbaa !24
  %208 = shl i32 %205, 31
  %209 = icmp eq i64 %203, 0
  br i1 %209, label %191, label %200
210:                                              ; preds = %191, %197, %186
  %211 = add nsw i32 %111, -1
  %212 = icmp sgt i32 %111, 0
  br i1 %212, label %109, label %98
213:                                              ; preds = %106, %100, %21
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %4) #10
  ret void
}

; Function Attrs: nounwind memory(argmem: readwrite, inaccessiblemem: write)
define weak hidden i32 @tz_soft_format(ptr noundef writeonly %0, ptr noundef readonly %1, i32 noundef %2) local_unnamed_addr #2 {
  %4 = alloca [12 x i8], align 1
  %5 = alloca %struct.tzrt_big, align 4
  %6 = alloca %struct.tzrt_big, align 4
  %7 = alloca %struct.tzrt_big, align 4
  %8 = alloca %struct.tzrt_big, align 4
  %9 = alloca %struct.tzrt_big, align 4
  %10 = alloca %struct.tzrt_big, align 4
  %11 = alloca %struct.tzrt_format, align 8
  %12 = alloca [40 x i8], align 16
  %13 = alloca %struct.tzrt_number, align 4
  %14 = alloca [48 x i8], align 16
  call void @llvm.lifetime.start.p0(i64 36, ptr nonnull %11) #10
  switch i32 %2, label %78 [
    i32 0, label %15
    i32 1, label %24
    i32 2, label %33
    i32 3, label %42
    i32 4, label %51
    i32 5, label %60
    i32 6, label %69
  ]

15:                                               ; preds = %3
  store i32 2, ptr %11, align 8, !tbaa !4, !alias.scope !99
  %16 = getelementptr inbounds i8, ptr %11, i64 4
  store i32 11, ptr %16, align 4, !tbaa !12, !alias.scope !99
  %17 = getelementptr inbounds i8, ptr %11, i64 8
  store i32 -24, ptr %17, align 8, !tbaa !13, !alias.scope !99
  %18 = getelementptr inbounds i8, ptr %11, i64 12
  store i32 5, ptr %18, align 4, !tbaa !14, !alias.scope !99
  %19 = getelementptr inbounds i8, ptr %11, i64 16
  store i32 10, ptr %19, align 8, !tbaa !15, !alias.scope !99
  %20 = getelementptr inbounds i8, ptr %11, i64 20
  store i32 15, ptr %20, align 4, !tbaa !16, !alias.scope !99
  %21 = getelementptr inbounds i8, ptr %11, i64 24
  store i32 16, ptr %21, align 8, !tbaa !17, !alias.scope !99
  %22 = getelementptr inbounds i8, ptr %11, i64 28
  store i32 0, ptr %22, align 4, !tbaa !18, !alias.scope !99
  %23 = getelementptr inbounds i8, ptr %11, i64 32
  store i32 1, ptr %23, align 8, !tbaa !19, !alias.scope !99
  br label %91

24:                                               ; preds = %3
  store i32 2, ptr %11, align 8, !tbaa !4, !alias.scope !99
  %25 = getelementptr inbounds i8, ptr %11, i64 4
  store i32 24, ptr %25, align 4, !tbaa !12, !alias.scope !99
  %26 = getelementptr inbounds i8, ptr %11, i64 8
  store i32 -149, ptr %26, align 8, !tbaa !13, !alias.scope !99
  %27 = getelementptr inbounds i8, ptr %11, i64 12
  store i32 104, ptr %27, align 4, !tbaa !14, !alias.scope !99
  %28 = getelementptr inbounds i8, ptr %11, i64 16
  store i32 23, ptr %28, align 8, !tbaa !15, !alias.scope !99
  %29 = getelementptr inbounds i8, ptr %11, i64 20
  store i32 127, ptr %29, align 4, !tbaa !16, !alias.scope !99
  %30 = getelementptr inbounds i8, ptr %11, i64 24
  store i32 32, ptr %30, align 8, !tbaa !17, !alias.scope !99
  %31 = getelementptr inbounds i8, ptr %11, i64 28
  store i32 0, ptr %31, align 4, !tbaa !18, !alias.scope !99
  %32 = getelementptr inbounds i8, ptr %11, i64 32
  store i32 1, ptr %32, align 8, !tbaa !19, !alias.scope !99
  br label %91

33:                                               ; preds = %3
  store i32 2, ptr %11, align 8, !tbaa !4, !alias.scope !99
  %34 = getelementptr inbounds i8, ptr %11, i64 4
  store i32 53, ptr %34, align 4, !tbaa !12, !alias.scope !99
  %35 = getelementptr inbounds i8, ptr %11, i64 8
  store i32 -1074, ptr %35, align 8, !tbaa !13, !alias.scope !99
  %36 = getelementptr inbounds i8, ptr %11, i64 12
  store i32 971, ptr %36, align 4, !tbaa !14, !alias.scope !99
  %37 = getelementptr inbounds i8, ptr %11, i64 16
  store i32 52, ptr %37, align 8, !tbaa !15, !alias.scope !99
  %38 = getelementptr inbounds i8, ptr %11, i64 20
  store i32 1023, ptr %38, align 4, !tbaa !16, !alias.scope !99
  %39 = getelementptr inbounds i8, ptr %11, i64 24
  store i32 64, ptr %39, align 8, !tbaa !17, !alias.scope !99
  %40 = getelementptr inbounds i8, ptr %11, i64 28
  store i32 0, ptr %40, align 4, !tbaa !18, !alias.scope !99
  %41 = getelementptr inbounds i8, ptr %11, i64 32
  store i32 1, ptr %41, align 8, !tbaa !19, !alias.scope !99
  br label %91

42:                                               ; preds = %3
  store i32 2, ptr %11, align 8, !tbaa !4, !alias.scope !99
  %43 = getelementptr inbounds i8, ptr %11, i64 4
  store i32 113, ptr %43, align 4, !tbaa !12, !alias.scope !99
  %44 = getelementptr inbounds i8, ptr %11, i64 8
  store i32 -16494, ptr %44, align 8, !tbaa !13, !alias.scope !99
  %45 = getelementptr inbounds i8, ptr %11, i64 12
  store i32 16271, ptr %45, align 4, !tbaa !14, !alias.scope !99
  %46 = getelementptr inbounds i8, ptr %11, i64 16
  store i32 112, ptr %46, align 8, !tbaa !15, !alias.scope !99
  %47 = getelementptr inbounds i8, ptr %11, i64 20
  store i32 16383, ptr %47, align 4, !tbaa !16, !alias.scope !99
  %48 = getelementptr inbounds i8, ptr %11, i64 24
  store i32 128, ptr %48, align 8, !tbaa !17, !alias.scope !99
  %49 = getelementptr inbounds i8, ptr %11, i64 28
  store i32 0, ptr %49, align 4, !tbaa !18, !alias.scope !99
  %50 = getelementptr inbounds i8, ptr %11, i64 32
  store i32 1, ptr %50, align 8, !tbaa !19, !alias.scope !99
  br label %91

51:                                               ; preds = %3
  store i32 10, ptr %11, align 8, !tbaa !4, !alias.scope !99
  %52 = getelementptr inbounds i8, ptr %11, i64 4
  store i32 7, ptr %52, align 4, !tbaa !12, !alias.scope !99
  %53 = getelementptr inbounds i8, ptr %11, i64 8
  store i32 -101, ptr %53, align 8, !tbaa !13, !alias.scope !99
  %54 = getelementptr inbounds i8, ptr %11, i64 12
  store i32 90, ptr %54, align 4, !tbaa !14, !alias.scope !99
  %55 = getelementptr inbounds i8, ptr %11, i64 16
  store i32 23, ptr %55, align 8, !tbaa !15, !alias.scope !99
  %56 = getelementptr inbounds i8, ptr %11, i64 20
  store i32 101, ptr %56, align 4, !tbaa !16, !alias.scope !99
  %57 = getelementptr inbounds i8, ptr %11, i64 24
  store i32 32, ptr %57, align 8, !tbaa !17, !alias.scope !99
  %58 = getelementptr inbounds i8, ptr %11, i64 28
  store i32 0, ptr %58, align 4, !tbaa !18, !alias.scope !99
  %59 = getelementptr inbounds i8, ptr %11, i64 32
  store i32 1, ptr %59, align 8, !tbaa !19, !alias.scope !99
  br label %91

60:                                               ; preds = %3
  store i32 10, ptr %11, align 8, !tbaa !4, !alias.scope !99
  %61 = getelementptr inbounds i8, ptr %11, i64 4
  store i32 16, ptr %61, align 4, !tbaa !12, !alias.scope !99
  %62 = getelementptr inbounds i8, ptr %11, i64 8
  store i32 -398, ptr %62, align 8, !tbaa !13, !alias.scope !99
  %63 = getelementptr inbounds i8, ptr %11, i64 12
  store i32 369, ptr %63, align 4, !tbaa !14, !alias.scope !99
  %64 = getelementptr inbounds i8, ptr %11, i64 16
  store i32 53, ptr %64, align 8, !tbaa !15, !alias.scope !99
  %65 = getelementptr inbounds i8, ptr %11, i64 20
  store i32 398, ptr %65, align 4, !tbaa !16, !alias.scope !99
  %66 = getelementptr inbounds i8, ptr %11, i64 24
  store i32 64, ptr %66, align 8, !tbaa !17, !alias.scope !99
  %67 = getelementptr inbounds i8, ptr %11, i64 28
  store i32 0, ptr %67, align 4, !tbaa !18, !alias.scope !99
  %68 = getelementptr inbounds i8, ptr %11, i64 32
  store i32 1, ptr %68, align 8, !tbaa !19, !alias.scope !99
  br label %91

69:                                               ; preds = %3
  store i32 10, ptr %11, align 8, !tbaa !4, !alias.scope !99
  %70 = getelementptr inbounds i8, ptr %11, i64 4
  store i32 34, ptr %70, align 4, !tbaa !12, !alias.scope !99
  %71 = getelementptr inbounds i8, ptr %11, i64 8
  store i32 -6176, ptr %71, align 8, !tbaa !13, !alias.scope !99
  %72 = getelementptr inbounds i8, ptr %11, i64 12
  store i32 6111, ptr %72, align 4, !tbaa !14, !alias.scope !99
  %73 = getelementptr inbounds i8, ptr %11, i64 16
  store i32 113, ptr %73, align 8, !tbaa !15, !alias.scope !99
  %74 = getelementptr inbounds i8, ptr %11, i64 20
  store i32 6176, ptr %74, align 4, !tbaa !16, !alias.scope !99
  %75 = getelementptr inbounds i8, ptr %11, i64 24
  store i32 128, ptr %75, align 8, !tbaa !17, !alias.scope !99
  %76 = getelementptr inbounds i8, ptr %11, i64 28
  store i32 0, ptr %76, align 4, !tbaa !18, !alias.scope !99
  %77 = getelementptr inbounds i8, ptr %11, i64 32
  store i32 1, ptr %77, align 8, !tbaa !19, !alias.scope !99
  br label %91

78:                                               ; preds = %3
  %79 = and i32 %2, 7
  %80 = shl nuw nsw i32 8, %79
  store i32 2, ptr %11, align 8, !tbaa !4, !alias.scope !99
  %81 = getelementptr inbounds i8, ptr %11, i64 4
  store i32 %80, ptr %81, align 4, !tbaa !12, !alias.scope !99
  %82 = getelementptr inbounds i8, ptr %11, i64 8
  store i32 0, ptr %82, align 8, !tbaa !13, !alias.scope !99
  %83 = getelementptr inbounds i8, ptr %11, i64 12
  store i32 0, ptr %83, align 4, !tbaa !14, !alias.scope !99
  %84 = getelementptr inbounds i8, ptr %11, i64 16
  store i32 0, ptr %84, align 8, !tbaa !15, !alias.scope !99
  %85 = getelementptr inbounds i8, ptr %11, i64 20
  store i32 0, ptr %85, align 4, !tbaa !16, !alias.scope !99
  %86 = getelementptr inbounds i8, ptr %11, i64 24
  store i32 %80, ptr %86, align 8, !tbaa !17, !alias.scope !99
  %87 = getelementptr inbounds i8, ptr %11, i64 28
  store i32 1, ptr %87, align 4, !tbaa !18, !alias.scope !99
  %88 = getelementptr inbounds i8, ptr %11, i64 32
  %89 = icmp slt i32 %2, 24
  %90 = zext i1 %89 to i32
  store i32 %90, ptr %88, align 8, !tbaa !19, !alias.scope !99
  br label %91

91:                                               ; preds = %15, %24, %33, %42, %51, %60, %69, %78
  %92 = phi i1 [ true, %15 ], [ true, %24 ], [ true, %33 ], [ true, %42 ], [ true, %51 ], [ true, %60 ], [ true, %69 ], [ %89, %78 ]
  %93 = phi i1 [ true, %15 ], [ true, %24 ], [ true, %33 ], [ true, %42 ], [ true, %51 ], [ true, %60 ], [ true, %69 ], [ false, %78 ]
  %94 = phi i32 [ 16, %15 ], [ 32, %24 ], [ 64, %33 ], [ 128, %42 ], [ 32, %51 ], [ 64, %60 ], [ 128, %69 ], [ %80, %78 ]
  %95 = phi i1 [ true, %15 ], [ true, %24 ], [ true, %33 ], [ true, %42 ], [ false, %51 ], [ false, %60 ], [ false, %69 ], [ true, %78 ]
  %96 = phi i32 [ 10, %15 ], [ 23, %24 ], [ 52, %33 ], [ 112, %42 ], [ 23, %51 ], [ 53, %60 ], [ 113, %69 ], [ 0, %78 ]
  %97 = phi i32 [ -24, %15 ], [ -149, %24 ], [ -1074, %33 ], [ -16494, %42 ], [ -101, %51 ], [ -398, %60 ], [ -6176, %69 ], [ 0, %78 ]
  br i1 %93, label %160, label %98

98:                                               ; preds = %91
  %99 = lshr exact i32 %94, 3
  %100 = zext i32 %99 to i64
  br label %101

101:                                              ; preds = %101, %98
  %102 = phi i64 [ %100, %98 ], [ %104, %101 ]
  %103 = phi i128 [ 0, %98 ], [ %109, %101 ]
  %104 = add nsw i64 %102, -1
  %105 = shl i128 %103, 8
  %106 = getelementptr inbounds i8, ptr %1, i64 %104
  %107 = load i8, ptr %106, align 1, !tbaa !26
  %108 = zext i8 %107 to i128
  %109 = or i128 %105, %108
  %110 = icmp eq i64 %104, 0
  br i1 %110, label %111, label %101
111:                                              ; preds = %101
  %112 = add nsw i32 %94, -1
  %113 = zext i32 %112 to i128
  %114 = lshr i128 %109, %113
  %115 = icmp ne i128 %114, 0
  %116 = select i1 %92, i1 %115, i1 false
  br i1 %116, label %117, label %125

117:                                              ; preds = %111
  %118 = sub i128 0, %109
  %119 = icmp eq i32 %94, 128
  %120 = zext i32 %94 to i128
  %121 = shl nsw i128 -1, %120
  %122 = xor i128 %121, -1
  %123 = select i1 %119, i128 -1, i128 %122
  %124 = and i128 %123, %118
  br label %125

125:                                              ; preds = %117, %111
  %126 = phi i128 [ %124, %117 ], [ %109, %111 ]
  call void @llvm.lifetime.start.p0(i64 40, ptr nonnull %12) #10
  br label %127

127:                                              ; preds = %127, %125
  %128 = phi i32 [ %140, %127 ], [ 1, %125 ]
  %129 = phi i64 [ %137, %127 ], [ 0, %125 ]
  %130 = phi i128 [ %132, %127 ], [ %126, %125 ]
  %131 = freeze i128 %130
  %132 = udiv i128 %131, 10
  %133 = mul i128 %132, 10
  %134 = sub i128 %131, %133
  %135 = trunc i128 %134 to i8
  %136 = or i8 %135, 48
  %137 = add nuw nsw i64 %129, 1
  %138 = getelementptr inbounds [40 x i8], ptr %12, i64 0, i64 %129
  store i8 %136, ptr %138, align 1, !tbaa !26
  %139 = icmp ult i128 %130, 10
  %140 = add nuw i32 %128, 1
  br i1 %139, label %141, label %127
141:                                              ; preds = %127
  br i1 %116, label %142, label %143

142:                                              ; preds = %141
  store i8 45, ptr %0, align 1, !tbaa !26
  br label %143

143:                                              ; preds = %142, %141
  %144 = phi i32 [ 1, %142 ], [ 0, %141 ]
  %145 = sext i32 %128 to i64
  %146 = zext i32 %144 to i64
  %147 = add i32 %144, %128
  %148 = zext i32 %147 to i64
  br label %149

149:                                              ; preds = %143, %149
  %150 = phi i64 [ %146, %143 ], [ %155, %149 ]
  %151 = phi i64 [ %145, %143 ], [ %152, %149 ]
  %152 = add nsw i64 %151, -1
  %153 = getelementptr inbounds [40 x i8], ptr %12, i64 0, i64 %152
  %154 = load i8, ptr %153, align 1, !tbaa !26
  %155 = add nuw nsw i64 %150, 1
  %156 = getelementptr inbounds i8, ptr %0, i64 %150
  store i8 %154, ptr %156, align 1, !tbaa !26
  %157 = icmp eq i64 %155, %148
  br i1 %157, label %158, label %149
158:                                              ; preds = %149
  %159 = trunc i64 %155 to i32
  call void @llvm.lifetime.end.p0(i64 40, ptr nonnull %12) #10
  br label %1262

160:                                              ; preds = %91
  call void @llvm.lifetime.start.p0(i64 5616, ptr nonnull %13) #10
  call fastcc void @decode(ptr sret(%struct.tzrt_number) align 4 %13, ptr noundef %1, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %11) #11
  %161 = getelementptr inbounds i8, ptr %13, i64 5608
  %162 = load i32, ptr %161, align 4, !tbaa !20
  %163 = icmp ne i32 %162, 0
  %164 = getelementptr inbounds i8, ptr %13, i64 5612
  %165 = load i32, ptr %164, align 4
  %166 = icmp ne i32 %165, 2
  %167 = select i1 %163, i1 %166, i1 false
  br i1 %167, label %168, label %169

168:                                              ; preds = %160
  store i8 45, ptr %0, align 1, !tbaa !26
  br label %169

169:                                              ; preds = %168, %160
  %170 = phi i32 [ 1, %168 ], [ 0, %160 ]
  %171 = icmp eq i32 %165, 0
  br i1 %171, label %185, label %172

172:                                              ; preds = %169
  %173 = icmp eq i32 %165, 2
  %174 = select i1 %173, ptr @.str, ptr @.str.1
  %175 = zext i32 %170 to i64
  br label %176

176:                                              ; preds = %172, %176
  %177 = phi i64 [ %175, %172 ], [ %181, %176 ]
  %178 = phi i64 [ 0, %172 ], [ %183, %176 ]
  %179 = getelementptr inbounds i8, ptr %174, i64 %178
  %180 = load i8, ptr %179, align 1, !tbaa !26
  %181 = add nuw nsw i64 %177, 1
  %182 = getelementptr inbounds i8, ptr %0, i64 %177
  store i8 %180, ptr %182, align 1, !tbaa !26
  %183 = add nuw nsw i64 %178, 1
  %184 = icmp eq i64 %183, 3
  br i1 %184, label %1258, label %176
185:                                              ; preds = %169
  %186 = load i32, ptr %13, align 4, !tbaa !25
  %187 = icmp eq i32 %186, 0
  br i1 %187, label %188, label %192

188:                                              ; preds = %185
  %189 = add nuw nsw i32 %170, 1
  %190 = zext i32 %170 to i64
  %191 = getelementptr inbounds i8, ptr %0, i64 %190
  store i8 48, ptr %191, align 1, !tbaa !26
  br label %1260

192:                                              ; preds = %185
  br i1 %95, label %193, label %1046

193:                                              ; preds = %192
  %194 = icmp sgt i32 %186, 4
  br i1 %194, label %198, label %195

195:                                              ; preds = %193
  %196 = getelementptr inbounds i8, ptr %13, i64 4
  %197 = sext i32 %186 to i64
  br label %204

198:                                              ; preds = %193
  tail call void @llvm.trap()
  unreachable

199:                                              ; preds = %204
  %200 = and i128 %211, 1
  %201 = icmp eq i128 %200, 0
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %5) #10
  %202 = shl i128 %212, 2
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %5, i8 0, i64 5604, i1 false), !alias.scope !105
  %203 = icmp eq i128 %202, 0
  br i1 %203, label %225, label %214

204:                                              ; preds = %204, %195
  %205 = phi i64 [ %197, %195 ], [ %207, %204 ]
  %206 = phi i128 [ 0, %195 ], [ %212, %204 ]
  %207 = add nsw i64 %205, -1
  %208 = shl i128 %206, 32
  %209 = getelementptr inbounds [1400 x i32], ptr %196, i64 0, i64 %207
  %210 = load i32, ptr %209, align 4, !tbaa !24
  %211 = zext i32 %210 to i128
  %212 = or i128 %208, %211
  %213 = icmp eq i64 %207, 0
  br i1 %213, label %199, label %204
214:                                              ; preds = %199
  %215 = getelementptr inbounds i8, ptr %5, i64 4
  br label %216

216:                                              ; preds = %216, %214
  %217 = phi i128 [ %202, %214 ], [ %223, %216 ]
  %218 = trunc i128 %217 to i32
  %219 = load i32, ptr %5, align 4, !tbaa !34, !alias.scope !105
  %220 = add nsw i32 %219, 1
  store i32 %220, ptr %5, align 4, !tbaa !34, !alias.scope !105
  %221 = sext i32 %219 to i64
  %222 = getelementptr inbounds [1400 x i32], ptr %215, i64 0, i64 %221
  store i32 %218, ptr %222, align 4, !tbaa !24, !alias.scope !105
  %223 = lshr i128 %217, 32
  %224 = icmp ult i128 %217, 4294967296
  br i1 %224, label %225, label %216
225:                                              ; preds = %216, %199
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %6) #10
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %6, i8 0, i64 5604, i1 false), !alias.scope !108
  %226 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 1, ptr %6, align 4, !tbaa !34, !alias.scope !108
  store i32 4, ptr %226, align 4, !tbaa !24, !alias.scope !108
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %7) #10
  %227 = zext i32 %96 to i128
  %228 = shl nuw nsw i128 1, %227
  %229 = icmp eq i128 %212, %228
  %230 = getelementptr inbounds i8, ptr %13, i64 5604
  %231 = load i32, ptr %230, align 4
  %232 = icmp sgt i32 %231, %97
  %233 = select i1 %229, i1 %232, i1 false
  %234 = select i1 %233, i32 1, i32 2
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %7, i8 0, i64 5604, i1 false), !alias.scope !111
  %235 = getelementptr inbounds i8, ptr %7, i64 4
  store i32 1, ptr %7, align 4, !tbaa !34, !alias.scope !111
  store i32 %234, ptr %235, align 4, !tbaa !24, !alias.scope !111
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %8) #10
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %8, i8 0, i64 5604, i1 false), !alias.scope !114
  %236 = getelementptr inbounds i8, ptr %8, i64 4
  store i32 1, ptr %8, align 4, !tbaa !34, !alias.scope !114
  store i32 2, ptr %236, align 4, !tbaa !24, !alias.scope !114
  %237 = load i32, ptr %230, align 4, !tbaa !35
  %238 = icmp sgt i32 %237, -1
  br i1 %238, label %239, label %369

239:                                              ; preds = %225
  %240 = load i32, ptr %5, align 4, !tbaa !34
  %241 = icmp ne i32 %240, 0
  %242 = icmp ne i32 %237, 0
  %243 = and i1 %242, %241
  br i1 %243, label %244, label %296

244:                                              ; preds = %239
  %245 = lshr i32 %237, 5
  %246 = and i32 %237, 31
  %247 = add nsw i32 %240, %245
  %248 = icmp sgt i32 %247, 1399
  br i1 %248, label %253, label %249

249:                                              ; preds = %244
  %250 = getelementptr inbounds i8, ptr %5, i64 4
  %251 = sext i32 %240 to i64
  %252 = zext i32 %245 to i64
  br label %256

253:                                              ; preds = %244
  tail call void @llvm.trap()
  unreachable

254:                                              ; preds = %256
  %255 = icmp sgt i32 %237, 31
  br i1 %255, label %271, label %264

256:                                              ; preds = %256, %249
  %257 = phi i64 [ %251, %249 ], [ %258, %256 ]
  %258 = add nsw i64 %257, -1
  %259 = getelementptr inbounds [1400 x i32], ptr %250, i64 0, i64 %258
  %260 = load i32, ptr %259, align 4, !tbaa !24
  %261 = add nsw i64 %258, %252
  %262 = getelementptr inbounds [1400 x i32], ptr %250, i64 0, i64 %261
  store i32 %260, ptr %262, align 4, !tbaa !24
  %263 = icmp eq i64 %258, 0
  br i1 %263, label %254, label %256
264:                                              ; preds = %271, %254
  %265 = load i32, ptr %5, align 4, !tbaa !34
  %266 = add nsw i32 %265, %245
  store i32 %266, ptr %5, align 4, !tbaa !34
  %267 = icmp sgt i32 %265, 0
  br i1 %267, label %268, label %276

268:                                              ; preds = %264
  %269 = zext i32 %246 to i64
  %270 = zext i32 %266 to i64
  br label %279

271:                                              ; preds = %254, %271
  %272 = phi i64 [ %274, %271 ], [ 0, %254 ]
  %273 = getelementptr inbounds [1400 x i32], ptr %250, i64 0, i64 %272
  store i32 0, ptr %273, align 4, !tbaa !24
  %274 = add nuw nsw i64 %272, 1
  %275 = icmp eq i64 %274, %252
  br i1 %275, label %264, label %271
276:                                              ; preds = %279, %264
  %277 = phi i64 [ 0, %264 ], [ %288, %279 ]
  %278 = icmp eq i64 %277, 0
  br i1 %278, label %296, label %291

279:                                              ; preds = %279, %268
  %280 = phi i64 [ %252, %268 ], [ %289, %279 ]
  %281 = phi i64 [ 0, %268 ], [ %288, %279 ]
  %282 = getelementptr inbounds [1400 x i32], ptr %250, i64 0, i64 %280
  %283 = load i32, ptr %282, align 4, !tbaa !24
  %284 = zext i32 %283 to i64
  %285 = shl nuw nsw i64 %284, %269
  %286 = or i64 %285, %281
  %287 = trunc i64 %286 to i32
  store i32 %287, ptr %282, align 4, !tbaa !24
  %288 = lshr i64 %285, 32
  %289 = add nuw nsw i64 %280, 1
  %290 = icmp samesign ult i64 %289, %270
  br i1 %290, label %279, label %276
291:                                              ; preds = %276
  %292 = trunc i64 %277 to i32
  %293 = add nsw i32 %266, 1
  store i32 %293, ptr %5, align 4, !tbaa !34
  %294 = sext i32 %266 to i64
  %295 = getelementptr inbounds [1400 x i32], ptr %250, i64 0, i64 %294
  store i32 %292, ptr %295, align 4, !tbaa !24
  br label %296

296:                                              ; preds = %291, %276, %239
  br i1 %242, label %297, label %416

297:                                              ; preds = %296
  %298 = lshr i32 %237, 5
  %299 = and i32 %237, 31
  %300 = icmp ugt i32 %237, 44767
  br i1 %300, label %306, label %301

301:                                              ; preds = %297
  %302 = zext i32 %298 to i64
  %303 = load i32, ptr %235, align 4, !tbaa !24
  %304 = getelementptr inbounds [1400 x i32], ptr %235, i64 0, i64 %302
  store i32 %303, ptr %304, align 4, !tbaa !24
  %305 = icmp sgt i32 %237, 31
  br i1 %305, label %314, label %307

306:                                              ; preds = %297
  tail call void @llvm.trap()
  unreachable

307:                                              ; preds = %314, %301
  %308 = load i32, ptr %7, align 4, !tbaa !34
  %309 = add nsw i32 %308, %298
  store i32 %309, ptr %7, align 4, !tbaa !34
  %310 = icmp sgt i32 %308, 0
  br i1 %310, label %311, label %319

311:                                              ; preds = %307
  %312 = zext i32 %299 to i64
  %313 = zext i32 %309 to i64
  br label %322

314:                                              ; preds = %301, %314
  %315 = phi i64 [ %317, %314 ], [ 0, %301 ]
  %316 = getelementptr inbounds [1400 x i32], ptr %235, i64 0, i64 %315
  store i32 0, ptr %316, align 4, !tbaa !24
  %317 = add nuw nsw i64 %315, 1
  %318 = icmp eq i64 %317, %302
  br i1 %318, label %307, label %314
319:                                              ; preds = %322, %307
  %320 = phi i64 [ 0, %307 ], [ %331, %322 ]
  %321 = icmp eq i64 %320, 0
  br i1 %321, label %339, label %334

322:                                              ; preds = %322, %311
  %323 = phi i64 [ %302, %311 ], [ %332, %322 ]
  %324 = phi i64 [ 0, %311 ], [ %331, %322 ]
  %325 = getelementptr inbounds [1400 x i32], ptr %235, i64 0, i64 %323
  %326 = load i32, ptr %325, align 4, !tbaa !24
  %327 = zext i32 %326 to i64
  %328 = shl nuw nsw i64 %327, %312
  %329 = or i64 %328, %324
  %330 = trunc i64 %329 to i32
  store i32 %330, ptr %325, align 4, !tbaa !24
  %331 = lshr i64 %328, 32
  %332 = add nuw nsw i64 %323, 1
  %333 = icmp samesign ult i64 %332, %313
  br i1 %333, label %322, label %319
334:                                              ; preds = %319
  %335 = trunc i64 %320 to i32
  %336 = add nsw i32 %309, 1
  store i32 %336, ptr %7, align 4, !tbaa !34
  %337 = sext i32 %309 to i64
  %338 = getelementptr inbounds [1400 x i32], ptr %235, i64 0, i64 %337
  store i32 %335, ptr %338, align 4, !tbaa !24
  br label %339

339:                                              ; preds = %319, %334
  %340 = load i32, ptr %236, align 4, !tbaa !24
  %341 = getelementptr inbounds [1400 x i32], ptr %236, i64 0, i64 %302
  store i32 %340, ptr %341, align 4, !tbaa !24
  br i1 %305, label %349, label %342

342:                                              ; preds = %349, %339
  %343 = load i32, ptr %8, align 4, !tbaa !34
  %344 = add nsw i32 %343, %298
  store i32 %344, ptr %8, align 4, !tbaa !34
  %345 = icmp sgt i32 %343, 0
  br i1 %345, label %346, label %354

346:                                              ; preds = %342
  %347 = zext i32 %299 to i64
  %348 = zext i32 %344 to i64
  br label %357

349:                                              ; preds = %339, %349
  %350 = phi i64 [ %352, %349 ], [ 0, %339 ]
  %351 = getelementptr inbounds [1400 x i32], ptr %236, i64 0, i64 %350
  store i32 0, ptr %351, align 4, !tbaa !24
  %352 = add nuw nsw i64 %350, 1
  %353 = icmp eq i64 %352, %302
  br i1 %353, label %342, label %349
354:                                              ; preds = %357, %342
  %355 = phi i64 [ 0, %342 ], [ %366, %357 ]
  %356 = icmp eq i64 %355, 0
  br i1 %356, label %416, label %407

357:                                              ; preds = %357, %346
  %358 = phi i64 [ %302, %346 ], [ %367, %357 ]
  %359 = phi i64 [ 0, %346 ], [ %366, %357 ]
  %360 = getelementptr inbounds [1400 x i32], ptr %236, i64 0, i64 %358
  %361 = load i32, ptr %360, align 4, !tbaa !24
  %362 = zext i32 %361 to i64
  %363 = shl nuw nsw i64 %362, %347
  %364 = or i64 %363, %359
  %365 = trunc i64 %364 to i32
  store i32 %365, ptr %360, align 4, !tbaa !24
  %366 = lshr i64 %363, 32
  %367 = add nuw nsw i64 %358, 1
  %368 = icmp samesign ult i64 %367, %348
  br i1 %368, label %357, label %354
369:                                              ; preds = %225
  %370 = sub nsw i32 0, %237
  %371 = sdiv i32 %237, -32
  %372 = and i32 %370, 31
  %373 = icmp slt i32 %237, -44767
  br i1 %373, label %379, label %374

374:                                              ; preds = %369
  %375 = zext i32 %371 to i64
  %376 = load i32, ptr %226, align 4, !tbaa !24
  %377 = getelementptr inbounds [1400 x i32], ptr %226, i64 0, i64 %375
  store i32 %376, ptr %377, align 4, !tbaa !24
  %378 = icmp slt i32 %237, -31
  br i1 %378, label %387, label %380

379:                                              ; preds = %369
  tail call void @llvm.trap()
  unreachable

380:                                              ; preds = %387, %374
  %381 = load i32, ptr %6, align 4, !tbaa !34
  %382 = add nsw i32 %381, %371
  store i32 %382, ptr %6, align 4, !tbaa !34
  %383 = icmp sgt i32 %381, 0
  br i1 %383, label %384, label %392

384:                                              ; preds = %380
  %385 = zext i32 %372 to i64
  %386 = zext i32 %382 to i64
  br label %395

387:                                              ; preds = %374, %387
  %388 = phi i64 [ %390, %387 ], [ 0, %374 ]
  %389 = getelementptr inbounds [1400 x i32], ptr %226, i64 0, i64 %388
  store i32 0, ptr %389, align 4, !tbaa !24
  %390 = add nuw nsw i64 %388, 1
  %391 = icmp eq i64 %390, %375
  br i1 %391, label %380, label %387
392:                                              ; preds = %395, %380
  %393 = phi i64 [ 0, %380 ], [ %404, %395 ]
  %394 = icmp eq i64 %393, 0
  br i1 %394, label %416, label %407

395:                                              ; preds = %395, %384
  %396 = phi i64 [ %375, %384 ], [ %405, %395 ]
  %397 = phi i64 [ 0, %384 ], [ %404, %395 ]
  %398 = getelementptr inbounds [1400 x i32], ptr %226, i64 0, i64 %396
  %399 = load i32, ptr %398, align 4, !tbaa !24
  %400 = zext i32 %399 to i64
  %401 = shl nuw nsw i64 %400, %385
  %402 = or i64 %401, %397
  %403 = trunc i64 %402 to i32
  store i32 %403, ptr %398, align 4, !tbaa !24
  %404 = lshr i64 %401, 32
  %405 = add nuw nsw i64 %396, 1
  %406 = icmp samesign ult i64 %405, %386
  br i1 %406, label %395, label %392
407:                                              ; preds = %392, %354
  %408 = phi i64 [ %355, %354 ], [ %393, %392 ]
  %409 = phi i32 [ %344, %354 ], [ %382, %392 ]
  %410 = phi ptr [ %8, %354 ], [ %6, %392 ]
  %411 = getelementptr inbounds i8, ptr %410, i64 4
  %412 = trunc i64 %408 to i32
  %413 = add nsw i32 %409, 1
  store i32 %413, ptr %410, align 4, !tbaa !34
  %414 = sext i32 %409 to i64
  %415 = getelementptr inbounds [1400 x i32], ptr %411, i64 0, i64 %414
  store i32 %412, ptr %415, align 4, !tbaa !24
  br label %416

416:                                              ; preds = %407, %296, %392, %354
  %417 = call fastcc i32 @magnitude(ptr noundef %5, ptr noundef %6, i32 noundef 10) #11
  %418 = icmp sgt i32 %417, -1
  br i1 %418, label %419, label %488

419:                                              ; preds = %416
  %420 = icmp sgt i32 %417, 8
  br i1 %420, label %424, label %421

421:                                              ; preds = %453, %419
  %422 = phi i32 [ %417, %419 ], [ %454, %453 ]
  %423 = icmp sgt i32 %422, 0
  br i1 %423, label %456, label %698

424:                                              ; preds = %419, %453
  %425 = phi i32 [ %454, %453 ], [ %417, %419 ]
  %426 = load i32, ptr %6, align 4, !tbaa !34
  %427 = icmp sgt i32 %426, 0
  br i1 %427, label %428, label %430

428:                                              ; preds = %424
  %429 = zext i32 %426 to i64
  br label %433

430:                                              ; preds = %433, %424
  %431 = phi i64 [ 0, %424 ], [ %442, %433 ]
  %432 = icmp eq i64 %431, 0
  br i1 %432, label %453, label %445

433:                                              ; preds = %433, %428
  %434 = phi i64 [ 0, %428 ], [ %443, %433 ]
  %435 = phi i64 [ 0, %428 ], [ %442, %433 ]
  %436 = getelementptr inbounds [1400 x i32], ptr %226, i64 0, i64 %434
  %437 = load i32, ptr %436, align 4, !tbaa !24
  %438 = zext i32 %437 to i64
  %439 = mul nuw nsw i64 %438, 1000000000
  %440 = add nuw nsw i64 %439, %435
  %441 = trunc i64 %440 to i32
  store i32 %441, ptr %436, align 4, !tbaa !24
  %442 = lshr i64 %440, 32
  %443 = add nuw nsw i64 %434, 1
  %444 = icmp eq i64 %443, %429
  br i1 %444, label %430, label %433
445:                                              ; preds = %430
  %446 = icmp eq i32 %426, 1400
  br i1 %446, label %447, label %448

447:                                              ; preds = %445
  tail call void @llvm.trap()
  unreachable

448:                                              ; preds = %445
  %449 = trunc i64 %431 to i32
  %450 = add nsw i32 %426, 1
  store i32 %450, ptr %6, align 4, !tbaa !34
  %451 = sext i32 %426 to i64
  %452 = getelementptr inbounds [1400 x i32], ptr %226, i64 0, i64 %451
  store i32 %449, ptr %452, align 4, !tbaa !24
  br label %453

453:                                              ; preds = %448, %430
  %454 = add nsw i32 %425, -9
  %455 = icmp sgt i32 %425, 17
  br i1 %455, label %424, label %421
456:                                              ; preds = %421, %486
  %457 = phi i32 [ %458, %486 ], [ %422, %421 ]
  %458 = add nsw i32 %457, -1
  %459 = load i32, ptr %6, align 4, !tbaa !34
  %460 = icmp sgt i32 %459, 0
  br i1 %460, label %461, label %463

461:                                              ; preds = %456
  %462 = zext i32 %459 to i64
  br label %466

463:                                              ; preds = %466, %456
  %464 = phi i64 [ 0, %456 ], [ %475, %466 ]
  %465 = icmp eq i64 %464, 0
  br i1 %465, label %486, label %478

466:                                              ; preds = %466, %461
  %467 = phi i64 [ 0, %461 ], [ %476, %466 ]
  %468 = phi i64 [ 0, %461 ], [ %475, %466 ]
  %469 = getelementptr inbounds [1400 x i32], ptr %226, i64 0, i64 %467
  %470 = load i32, ptr %469, align 4, !tbaa !24
  %471 = zext i32 %470 to i64
  %472 = mul nuw nsw i64 %471, 10
  %473 = add nuw nsw i64 %472, %468
  %474 = trunc i64 %473 to i32
  store i32 %474, ptr %469, align 4, !tbaa !24
  %475 = lshr i64 %473, 32
  %476 = add nuw nsw i64 %467, 1
  %477 = icmp eq i64 %476, %462
  br i1 %477, label %463, label %466
478:                                              ; preds = %463
  %479 = icmp eq i32 %459, 1400
  br i1 %479, label %480, label %481

480:                                              ; preds = %478
  tail call void @llvm.trap()
  unreachable

481:                                              ; preds = %478
  %482 = trunc i64 %464 to i32
  %483 = add nsw i32 %459, 1
  store i32 %483, ptr %6, align 4, !tbaa !34
  %484 = sext i32 %459 to i64
  %485 = getelementptr inbounds [1400 x i32], ptr %226, i64 0, i64 %484
  store i32 %482, ptr %485, align 4, !tbaa !24
  br label %486

486:                                              ; preds = %481, %463
  %487 = icmp sgt i32 %457, 1
  br i1 %487, label %456, label %698
488:                                              ; preds = %416
  %489 = sub nsw i32 0, %417
  %490 = icmp slt i32 %417, -8
  br i1 %490, label %491, label %493

491:                                              ; preds = %488
  %492 = getelementptr inbounds i8, ptr %5, i64 4
  br label %498

493:                                              ; preds = %527, %488
  %494 = phi i32 [ %489, %488 ], [ %528, %527 ]
  %495 = icmp sgt i32 %494, 0
  br i1 %495, label %496, label %562

496:                                              ; preds = %493
  %497 = getelementptr inbounds i8, ptr %5, i64 4
  br label %530

498:                                              ; preds = %527, %491
  %499 = phi i32 [ %489, %491 ], [ %528, %527 ]
  %500 = load i32, ptr %5, align 4, !tbaa !34
  %501 = icmp sgt i32 %500, 0
  br i1 %501, label %502, label %504

502:                                              ; preds = %498
  %503 = zext i32 %500 to i64
  br label %507

504:                                              ; preds = %507, %498
  %505 = phi i64 [ 0, %498 ], [ %516, %507 ]
  %506 = icmp eq i64 %505, 0
  br i1 %506, label %527, label %519

507:                                              ; preds = %507, %502
  %508 = phi i64 [ 0, %502 ], [ %517, %507 ]
  %509 = phi i64 [ 0, %502 ], [ %516, %507 ]
  %510 = getelementptr inbounds [1400 x i32], ptr %492, i64 0, i64 %508
  %511 = load i32, ptr %510, align 4, !tbaa !24
  %512 = zext i32 %511 to i64
  %513 = mul nuw nsw i64 %512, 1000000000
  %514 = add nuw nsw i64 %513, %509
  %515 = trunc i64 %514 to i32
  store i32 %515, ptr %510, align 4, !tbaa !24
  %516 = lshr i64 %514, 32
  %517 = add nuw nsw i64 %508, 1
  %518 = icmp eq i64 %517, %503
  br i1 %518, label %504, label %507
519:                                              ; preds = %504
  %520 = icmp eq i32 %500, 1400
  br i1 %520, label %521, label %522

521:                                              ; preds = %519
  tail call void @llvm.trap()
  unreachable

522:                                              ; preds = %519
  %523 = trunc i64 %505 to i32
  %524 = add nsw i32 %500, 1
  store i32 %524, ptr %5, align 4, !tbaa !34
  %525 = sext i32 %500 to i64
  %526 = getelementptr inbounds [1400 x i32], ptr %492, i64 0, i64 %525
  store i32 %523, ptr %526, align 4, !tbaa !24
  br label %527

527:                                              ; preds = %522, %504
  %528 = add nsw i32 %499, -9
  %529 = icmp sgt i32 %499, 17
  br i1 %529, label %498, label %493
530:                                              ; preds = %560, %496
  %531 = phi i32 [ %494, %496 ], [ %532, %560 ]
  %532 = add nsw i32 %531, -1
  %533 = load i32, ptr %5, align 4, !tbaa !34
  %534 = icmp sgt i32 %533, 0
  br i1 %534, label %535, label %537

535:                                              ; preds = %530
  %536 = zext i32 %533 to i64
  br label %540

537:                                              ; preds = %540, %530
  %538 = phi i64 [ 0, %530 ], [ %549, %540 ]
  %539 = icmp eq i64 %538, 0
  br i1 %539, label %560, label %552

540:                                              ; preds = %540, %535
  %541 = phi i64 [ 0, %535 ], [ %550, %540 ]
  %542 = phi i64 [ 0, %535 ], [ %549, %540 ]
  %543 = getelementptr inbounds [1400 x i32], ptr %497, i64 0, i64 %541
  %544 = load i32, ptr %543, align 4, !tbaa !24
  %545 = zext i32 %544 to i64
  %546 = mul nuw nsw i64 %545, 10
  %547 = add nuw nsw i64 %546, %542
  %548 = trunc i64 %547 to i32
  store i32 %548, ptr %543, align 4, !tbaa !24
  %549 = lshr i64 %547, 32
  %550 = add nuw nsw i64 %541, 1
  %551 = icmp eq i64 %550, %536
  br i1 %551, label %537, label %540
552:                                              ; preds = %537
  %553 = icmp eq i32 %533, 1400
  br i1 %553, label %554, label %555

554:                                              ; preds = %552
  tail call void @llvm.trap()
  unreachable

555:                                              ; preds = %552
  %556 = trunc i64 %538 to i32
  %557 = add nsw i32 %533, 1
  store i32 %557, ptr %5, align 4, !tbaa !34
  %558 = sext i32 %533 to i64
  %559 = getelementptr inbounds [1400 x i32], ptr %497, i64 0, i64 %558
  store i32 %556, ptr %559, align 4, !tbaa !24
  br label %560

560:                                              ; preds = %555, %537
  %561 = icmp sgt i32 %531, 1
  br i1 %561, label %530, label %562
562:                                              ; preds = %560, %493
  br i1 %490, label %566, label %563

563:                                              ; preds = %595, %562
  %564 = phi i32 [ %489, %562 ], [ %596, %595 ]
  %565 = icmp sgt i32 %564, 0
  br i1 %565, label %598, label %630

566:                                              ; preds = %562, %595
  %567 = phi i32 [ %596, %595 ], [ %489, %562 ]
  %568 = load i32, ptr %7, align 4, !tbaa !34
  %569 = icmp sgt i32 %568, 0
  br i1 %569, label %570, label %572

570:                                              ; preds = %566
  %571 = zext i32 %568 to i64
  br label %575

572:                                              ; preds = %575, %566
  %573 = phi i64 [ 0, %566 ], [ %584, %575 ]
  %574 = icmp eq i64 %573, 0
  br i1 %574, label %595, label %587

575:                                              ; preds = %575, %570
  %576 = phi i64 [ 0, %570 ], [ %585, %575 ]
  %577 = phi i64 [ 0, %570 ], [ %584, %575 ]
  %578 = getelementptr inbounds [1400 x i32], ptr %235, i64 0, i64 %576
  %579 = load i32, ptr %578, align 4, !tbaa !24
  %580 = zext i32 %579 to i64
  %581 = mul nuw nsw i64 %580, 1000000000
  %582 = add nuw nsw i64 %581, %577
  %583 = trunc i64 %582 to i32
  store i32 %583, ptr %578, align 4, !tbaa !24
  %584 = lshr i64 %582, 32
  %585 = add nuw nsw i64 %576, 1
  %586 = icmp eq i64 %585, %571
  br i1 %586, label %572, label %575
587:                                              ; preds = %572
  %588 = icmp eq i32 %568, 1400
  br i1 %588, label %589, label %590

589:                                              ; preds = %587
  tail call void @llvm.trap()
  unreachable

590:                                              ; preds = %587
  %591 = trunc i64 %573 to i32
  %592 = add nsw i32 %568, 1
  store i32 %592, ptr %7, align 4, !tbaa !34
  %593 = sext i32 %568 to i64
  %594 = getelementptr inbounds [1400 x i32], ptr %235, i64 0, i64 %593
  store i32 %591, ptr %594, align 4, !tbaa !24
  br label %595

595:                                              ; preds = %590, %572
  %596 = add nsw i32 %567, -9
  %597 = icmp sgt i32 %567, 17
  br i1 %597, label %566, label %563
598:                                              ; preds = %563, %628
  %599 = phi i32 [ %600, %628 ], [ %564, %563 ]
  %600 = add nsw i32 %599, -1
  %601 = load i32, ptr %7, align 4, !tbaa !34
  %602 = icmp sgt i32 %601, 0
  br i1 %602, label %603, label %605

603:                                              ; preds = %598
  %604 = zext i32 %601 to i64
  br label %608

605:                                              ; preds = %608, %598
  %606 = phi i64 [ 0, %598 ], [ %617, %608 ]
  %607 = icmp eq i64 %606, 0
  br i1 %607, label %628, label %620

608:                                              ; preds = %608, %603
  %609 = phi i64 [ 0, %603 ], [ %618, %608 ]
  %610 = phi i64 [ 0, %603 ], [ %617, %608 ]
  %611 = getelementptr inbounds [1400 x i32], ptr %235, i64 0, i64 %609
  %612 = load i32, ptr %611, align 4, !tbaa !24
  %613 = zext i32 %612 to i64
  %614 = mul nuw nsw i64 %613, 10
  %615 = add nuw nsw i64 %614, %610
  %616 = trunc i64 %615 to i32
  store i32 %616, ptr %611, align 4, !tbaa !24
  %617 = lshr i64 %615, 32
  %618 = add nuw nsw i64 %609, 1
  %619 = icmp eq i64 %618, %604
  br i1 %619, label %605, label %608
620:                                              ; preds = %605
  %621 = icmp eq i32 %601, 1400
  br i1 %621, label %622, label %623

622:                                              ; preds = %620
  tail call void @llvm.trap()
  unreachable

623:                                              ; preds = %620
  %624 = trunc i64 %606 to i32
  %625 = add nsw i32 %601, 1
  store i32 %625, ptr %7, align 4, !tbaa !34
  %626 = sext i32 %601 to i64
  %627 = getelementptr inbounds [1400 x i32], ptr %235, i64 0, i64 %626
  store i32 %624, ptr %627, align 4, !tbaa !24
  br label %628

628:                                              ; preds = %623, %605
  %629 = icmp sgt i32 %599, 1
  br i1 %629, label %598, label %630
630:                                              ; preds = %628, %563
  br i1 %490, label %634, label %631

631:                                              ; preds = %663, %630
  %632 = phi i32 [ %489, %630 ], [ %664, %663 ]
  %633 = icmp sgt i32 %632, 0
  br i1 %633, label %666, label %698

634:                                              ; preds = %630, %663
  %635 = phi i32 [ %664, %663 ], [ %489, %630 ]
  %636 = load i32, ptr %8, align 4, !tbaa !34
  %637 = icmp sgt i32 %636, 0
  br i1 %637, label %638, label %640

638:                                              ; preds = %634
  %639 = zext i32 %636 to i64
  br label %643

640:                                              ; preds = %643, %634
  %641 = phi i64 [ 0, %634 ], [ %652, %643 ]
  %642 = icmp eq i64 %641, 0
  br i1 %642, label %663, label %655

643:                                              ; preds = %643, %638
  %644 = phi i64 [ 0, %638 ], [ %653, %643 ]
  %645 = phi i64 [ 0, %638 ], [ %652, %643 ]
  %646 = getelementptr inbounds [1400 x i32], ptr %236, i64 0, i64 %644
  %647 = load i32, ptr %646, align 4, !tbaa !24
  %648 = zext i32 %647 to i64
  %649 = mul nuw nsw i64 %648, 1000000000
  %650 = add nuw nsw i64 %649, %645
  %651 = trunc i64 %650 to i32
  store i32 %651, ptr %646, align 4, !tbaa !24
  %652 = lshr i64 %650, 32
  %653 = add nuw nsw i64 %644, 1
  %654 = icmp eq i64 %653, %639
  br i1 %654, label %640, label %643
655:                                              ; preds = %640
  %656 = icmp eq i32 %636, 1400
  br i1 %656, label %657, label %658

657:                                              ; preds = %655
  tail call void @llvm.trap()
  unreachable

658:                                              ; preds = %655
  %659 = trunc i64 %641 to i32
  %660 = add nsw i32 %636, 1
  store i32 %660, ptr %8, align 4, !tbaa !34
  %661 = sext i32 %636 to i64
  %662 = getelementptr inbounds [1400 x i32], ptr %236, i64 0, i64 %661
  store i32 %659, ptr %662, align 4, !tbaa !24
  br label %663

663:                                              ; preds = %658, %640
  %664 = add nsw i32 %635, -9
  %665 = icmp sgt i32 %635, 17
  br i1 %665, label %634, label %631
666:                                              ; preds = %631, %696
  %667 = phi i32 [ %668, %696 ], [ %632, %631 ]
  %668 = add nsw i32 %667, -1
  %669 = load i32, ptr %8, align 4, !tbaa !34
  %670 = icmp sgt i32 %669, 0
  br i1 %670, label %671, label %673

671:                                              ; preds = %666
  %672 = zext i32 %669 to i64
  br label %676

673:                                              ; preds = %676, %666
  %674 = phi i64 [ 0, %666 ], [ %685, %676 ]
  %675 = icmp eq i64 %674, 0
  br i1 %675, label %696, label %688

676:                                              ; preds = %676, %671
  %677 = phi i64 [ 0, %671 ], [ %686, %676 ]
  %678 = phi i64 [ 0, %671 ], [ %685, %676 ]
  %679 = getelementptr inbounds [1400 x i32], ptr %236, i64 0, i64 %677
  %680 = load i32, ptr %679, align 4, !tbaa !24
  %681 = zext i32 %680 to i64
  %682 = mul nuw nsw i64 %681, 10
  %683 = add nuw nsw i64 %682, %678
  %684 = trunc i64 %683 to i32
  store i32 %684, ptr %679, align 4, !tbaa !24
  %685 = lshr i64 %683, 32
  %686 = add nuw nsw i64 %677, 1
  %687 = icmp eq i64 %686, %672
  br i1 %687, label %673, label %676
688:                                              ; preds = %673
  %689 = icmp eq i32 %669, 1400
  br i1 %689, label %690, label %691

690:                                              ; preds = %688
  tail call void @llvm.trap()
  unreachable

691:                                              ; preds = %688
  %692 = trunc i64 %674 to i32
  %693 = add nsw i32 %669, 1
  store i32 %693, ptr %8, align 4, !tbaa !34
  %694 = sext i32 %669 to i64
  %695 = getelementptr inbounds [1400 x i32], ptr %236, i64 0, i64 %694
  store i32 %692, ptr %695, align 4, !tbaa !24
  br label %696

696:                                              ; preds = %691, %673
  %697 = icmp sgt i32 %667, 1
  br i1 %697, label %666, label %698
698:                                              ; preds = %696, %486, %631, %421
  %699 = load i32, ptr %6, align 4, !tbaa !34
  %700 = getelementptr inbounds i8, ptr %5, i64 4
  %701 = sext i32 %699 to i64
  %702 = getelementptr inbounds i8, ptr %9, i64 4
  %703 = getelementptr inbounds i8, ptr %10, i64 4
  %704 = add i32 %417, 1
  %705 = load i32, ptr %230, align 4
  br label %706

706:                                              ; preds = %1041, %698
  %707 = phi i32 [ %705, %698 ], [ %1042, %1041 ]
  %708 = phi i32 [ 1, %698 ], [ %1044, %1041 ]
  %709 = phi i128 [ 0, %698 ], [ %1043, %1041 ]
  %710 = load i32, ptr %5, align 4, !tbaa !34
  br label %711

711:                                              ; preds = %777, %706
  %712 = phi i32 [ %710, %706 ], [ %778, %777 ]
  %713 = phi i32 [ 0, %706 ], [ %779, %777 ]
  %714 = icmp eq i32 %712, %699
  br i1 %714, label %715, label %719

715:                                              ; preds = %711
  %716 = icmp eq i32 %712, 0
  br i1 %716, label %735, label %717

717:                                              ; preds = %715
  %718 = sext i32 %712 to i64
  br label %724

719:                                              ; preds = %711
  %720 = icmp slt i32 %712, %699
  %721 = select i1 %720, i32 -1, i32 1
  br label %735

722:                                              ; preds = %724
  %723 = icmp eq i64 %726, 0
  br i1 %723, label %735, label %724
724:                                              ; preds = %717, %722
  %725 = phi i64 [ %718, %717 ], [ %726, %722 ]
  %726 = add nsw i64 %725, -1
  %727 = getelementptr inbounds [1400 x i32], ptr %700, i64 0, i64 %726
  %728 = load i32, ptr %727, align 4, !tbaa !24
  %729 = getelementptr inbounds [1400 x i32], ptr %226, i64 0, i64 %726
  %730 = load i32, ptr %729, align 4, !tbaa !24
  %731 = icmp eq i32 %728, %730
  br i1 %731, label %722, label %732
732:                                              ; preds = %724
  %733 = icmp ult i32 %728, %730
  %734 = select i1 %733, i32 -1, i32 1
  br label %735

735:                                              ; preds = %722, %715, %732, %719
  %736 = phi i32 [ %721, %719 ], [ %734, %732 ], [ 0, %715 ], [ 0, %722 ]
  %737 = icmp sgt i32 %736, -1
  br i1 %737, label %738, label %780

738:                                              ; preds = %735
  %739 = icmp sgt i32 %712, 0
  br i1 %739, label %740, label %742

740:                                              ; preds = %738
  %741 = zext i32 %712 to i64
  br label %755

742:                                              ; preds = %763, %738
  %743 = icmp eq i32 %712, 0
  br i1 %743, label %777, label %744

744:                                              ; preds = %742
  %745 = sext i32 %712 to i64
  br label %746

746:                                              ; preds = %752, %744
  %747 = phi i64 [ %745, %744 ], [ %748, %752 ]
  %748 = add nsw i64 %747, -1
  %749 = getelementptr inbounds [1400 x i32], ptr %700, i64 0, i64 %748
  %750 = load i32, ptr %749, align 4, !tbaa !24
  %751 = icmp eq i32 %750, 0
  br i1 %751, label %752, label %775

752:                                              ; preds = %746
  %753 = trunc i64 %748 to i32
  store i32 %753, ptr %5, align 4, !tbaa !34
  %754 = icmp eq i64 %748, 0
  br i1 %754, label %777, label %746
755:                                              ; preds = %763, %740
  %756 = phi i64 [ 0, %740 ], [ %773, %763 ]
  %757 = phi i64 [ 0, %740 ], [ %772, %763 ]
  %758 = icmp slt i64 %756, %701
  br i1 %758, label %759, label %763

759:                                              ; preds = %755
  %760 = getelementptr inbounds [1400 x i32], ptr %226, i64 0, i64 %756
  %761 = load i32, ptr %760, align 4, !tbaa !24
  %762 = zext i32 %761 to i64
  br label %763

763:                                              ; preds = %759, %755
  %764 = phi i64 [ %762, %759 ], [ 0, %755 ]
  %765 = add nuw nsw i64 %764, %757
  %766 = getelementptr inbounds [1400 x i32], ptr %700, i64 0, i64 %756
  %767 = load i32, ptr %766, align 4, !tbaa !24
  %768 = zext i32 %767 to i64
  %769 = trunc i64 %765 to i32
  %770 = sub i32 %767, %769
  store i32 %770, ptr %766, align 4, !tbaa !24
  %771 = icmp samesign ugt i64 %765, %768
  %772 = zext i1 %771 to i64
  %773 = add nuw nsw i64 %756, 1
  %774 = icmp eq i64 %773, %741
  br i1 %774, label %742, label %755
775:                                              ; preds = %746
  %776 = trunc i64 %747 to i32
  br label %777

777:                                              ; preds = %752, %775, %742
  %778 = phi i32 [ %712, %742 ], [ %776, %775 ], [ 0, %752 ]
  %779 = add i32 %713, 1
  br label %711
780:                                              ; preds = %735
  %781 = mul i128 %709, 10
  %782 = zext i32 %713 to i128
  %783 = add i128 %781, %782
  %784 = load i32, ptr %7, align 4, !tbaa !34
  %785 = icmp eq i32 %712, %784
  br i1 %785, label %786, label %790

786:                                              ; preds = %780
  %787 = icmp eq i32 %712, 0
  br i1 %787, label %806, label %788

788:                                              ; preds = %786
  %789 = sext i32 %712 to i64
  br label %795

790:                                              ; preds = %780
  %791 = icmp slt i32 %712, %784
  %792 = select i1 %791, i32 -1, i32 1
  br label %806

793:                                              ; preds = %795
  %794 = icmp eq i64 %797, 0
  br i1 %794, label %806, label %795
795:                                              ; preds = %788, %793
  %796 = phi i64 [ %789, %788 ], [ %797, %793 ]
  %797 = add nsw i64 %796, -1
  %798 = getelementptr inbounds [1400 x i32], ptr %700, i64 0, i64 %797
  %799 = load i32, ptr %798, align 4, !tbaa !24
  %800 = getelementptr inbounds [1400 x i32], ptr %235, i64 0, i64 %797
  %801 = load i32, ptr %800, align 4, !tbaa !24
  %802 = icmp eq i32 %799, %801
  br i1 %802, label %793, label %803
803:                                              ; preds = %795
  %804 = icmp ult i32 %799, %801
  %805 = select i1 %804, i32 -1, i32 1
  br label %806

806:                                              ; preds = %793, %786, %803, %790
  %807 = phi i32 [ %792, %790 ], [ %805, %803 ], [ 0, %786 ], [ 0, %793 ]
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %9) #10
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %9, ptr noundef nonnull align 4 dereferenceable(5604) %6, i64 5604, i1 false), !tbaa.struct !30
  %808 = load i32, ptr %9, align 4, !tbaa !34
  %809 = icmp sgt i32 %808, 0
  br i1 %809, label %810, label %813

810:                                              ; preds = %806
  %811 = zext i32 %808 to i64
  %812 = sext i32 %712 to i64
  br label %826

813:                                              ; preds = %834, %806
  %814 = icmp eq i32 %808, 0
  br i1 %814, label %846, label %815

815:                                              ; preds = %813
  %816 = sext i32 %808 to i64
  br label %817

817:                                              ; preds = %823, %815
  %818 = phi i64 [ %816, %815 ], [ %819, %823 ]
  %819 = add nsw i64 %818, -1
  %820 = getelementptr inbounds [1400 x i32], ptr %702, i64 0, i64 %819
  %821 = load i32, ptr %820, align 4, !tbaa !24
  %822 = icmp eq i32 %821, 0
  br i1 %822, label %823, label %846

823:                                              ; preds = %817
  %824 = trunc i64 %819 to i32
  store i32 %824, ptr %9, align 4, !tbaa !34
  %825 = icmp eq i64 %819, 0
  br i1 %825, label %846, label %817
826:                                              ; preds = %834, %810
  %827 = phi i64 [ 0, %810 ], [ %844, %834 ]
  %828 = phi i64 [ 0, %810 ], [ %843, %834 ]
  %829 = icmp slt i64 %827, %812
  br i1 %829, label %830, label %834

830:                                              ; preds = %826
  %831 = getelementptr inbounds [1400 x i32], ptr %700, i64 0, i64 %827
  %832 = load i32, ptr %831, align 4, !tbaa !24
  %833 = zext i32 %832 to i64
  br label %834

834:                                              ; preds = %830, %826
  %835 = phi i64 [ %833, %830 ], [ 0, %826 ]
  %836 = add nuw nsw i64 %835, %828
  %837 = getelementptr inbounds [1400 x i32], ptr %702, i64 0, i64 %827
  %838 = load i32, ptr %837, align 4, !tbaa !24
  %839 = zext i32 %838 to i64
  %840 = trunc i64 %836 to i32
  %841 = sub i32 %838, %840
  store i32 %841, ptr %837, align 4, !tbaa !24
  %842 = icmp samesign ugt i64 %836, %839
  %843 = zext i1 %842 to i64
  %844 = add nuw nsw i64 %827, 1
  %845 = icmp eq i64 %844, %811
  br i1 %845, label %813, label %826
846:                                              ; preds = %823, %817, %813
  %847 = load i32, ptr %9, align 4, !tbaa !34
  %848 = load i32, ptr %8, align 4, !tbaa !34
  %849 = icmp eq i32 %847, %848
  br i1 %849, label %850, label %854

850:                                              ; preds = %846
  %851 = icmp eq i32 %847, 0
  br i1 %851, label %870, label %852

852:                                              ; preds = %850
  %853 = sext i32 %847 to i64
  br label %859

854:                                              ; preds = %846
  %855 = icmp slt i32 %847, %848
  %856 = select i1 %855, i32 -1, i32 1
  br label %870

857:                                              ; preds = %859
  %858 = icmp eq i64 %861, 0
  br i1 %858, label %870, label %859
859:                                              ; preds = %852, %857
  %860 = phi i64 [ %853, %852 ], [ %861, %857 ]
  %861 = add nsw i64 %860, -1
  %862 = getelementptr inbounds [1400 x i32], ptr %702, i64 0, i64 %861
  %863 = load i32, ptr %862, align 4, !tbaa !24
  %864 = getelementptr inbounds [1400 x i32], ptr %236, i64 0, i64 %861
  %865 = load i32, ptr %864, align 4, !tbaa !24
  %866 = icmp eq i32 %863, %865
  br i1 %866, label %857, label %867
867:                                              ; preds = %859
  %868 = icmp ult i32 %863, %865
  %869 = select i1 %868, i32 -1, i32 1
  br label %870

870:                                              ; preds = %857, %850, %867, %854
  %871 = phi i32 [ %856, %854 ], [ %869, %867 ], [ 0, %850 ], [ 0, %857 ]
  %872 = icmp slt i32 %807, 0
  br i1 %872, label %876, label %873

873:                                              ; preds = %870
  %874 = icmp eq i32 %807, 0
  %875 = and i1 %201, %874
  br label %876

876:                                              ; preds = %873, %870
  %877 = phi i1 [ true, %870 ], [ %875, %873 ]
  %878 = icmp slt i32 %871, 0
  br i1 %878, label %882, label %879

879:                                              ; preds = %876
  %880 = icmp eq i32 %871, 0
  %881 = and i1 %201, %880
  br label %882

882:                                              ; preds = %879, %876
  %883 = phi i1 [ true, %876 ], [ %881, %879 ]
  %884 = or i1 %877, %883
  %885 = icmp sgt i32 %712, 0
  br i1 %884, label %886, label %961

886:                                              ; preds = %882
  br i1 %885, label %887, label %889

887:                                              ; preds = %886
  %888 = zext i32 %712 to i64
  br label %892

889:                                              ; preds = %892, %886
  %890 = phi i64 [ 0, %886 ], [ %901, %892 ]
  %891 = icmp eq i64 %890, 0
  br i1 %891, label %912, label %904

892:                                              ; preds = %892, %887
  %893 = phi i64 [ 0, %887 ], [ %902, %892 ]
  %894 = phi i64 [ 0, %887 ], [ %901, %892 ]
  %895 = getelementptr inbounds [1400 x i32], ptr %700, i64 0, i64 %893
  %896 = load i32, ptr %895, align 4, !tbaa !24
  %897 = zext i32 %896 to i64
  %898 = shl nuw nsw i64 %897, 1
  %899 = add nuw nsw i64 %898, %894
  %900 = trunc i64 %899 to i32
  store i32 %900, ptr %895, align 4, !tbaa !24
  %901 = lshr i64 %899, 32
  %902 = add nuw nsw i64 %893, 1
  %903 = icmp eq i64 %902, %888
  br i1 %903, label %889, label %892
904:                                              ; preds = %889
  %905 = icmp eq i32 %712, 1400
  br i1 %905, label %906, label %907

906:                                              ; preds = %904
  store i32 %707, ptr %230, align 4
  tail call void @llvm.trap()
  unreachable

907:                                              ; preds = %904
  %908 = trunc i64 %890 to i32
  %909 = add nsw i32 %712, 1
  store i32 %909, ptr %5, align 4, !tbaa !34
  %910 = sext i32 %712 to i64
  %911 = getelementptr inbounds [1400 x i32], ptr %700, i64 0, i64 %910
  store i32 %908, ptr %911, align 4, !tbaa !24
  br label %912

912:                                              ; preds = %907, %889
  %913 = load i32, ptr %5, align 4, !tbaa !34
  %914 = icmp eq i32 %913, %699
  br i1 %914, label %915, label %919

915:                                              ; preds = %912
  %916 = icmp eq i32 %913, 0
  br i1 %916, label %935, label %917

917:                                              ; preds = %915
  %918 = sext i32 %913 to i64
  br label %924

919:                                              ; preds = %912
  %920 = icmp slt i32 %913, %699
  %921 = select i1 %920, i32 -1, i32 1
  br label %935

922:                                              ; preds = %924
  %923 = icmp eq i64 %926, 0
  br i1 %923, label %935, label %924
924:                                              ; preds = %917, %922
  %925 = phi i64 [ %918, %917 ], [ %926, %922 ]
  %926 = add nsw i64 %925, -1
  %927 = getelementptr inbounds [1400 x i32], ptr %700, i64 0, i64 %926
  %928 = load i32, ptr %927, align 4, !tbaa !24
  %929 = getelementptr inbounds [1400 x i32], ptr %226, i64 0, i64 %926
  %930 = load i32, ptr %929, align 4, !tbaa !24
  %931 = icmp eq i32 %928, %930
  br i1 %931, label %922, label %932
932:                                              ; preds = %924
  %933 = icmp ult i32 %928, %930
  %934 = select i1 %933, i32 -1, i32 1
  br label %935

935:                                              ; preds = %922, %915, %932, %919
  %936 = phi i32 [ %921, %919 ], [ %934, %932 ], [ 0, %915 ], [ 0, %922 ]
  br i1 %883, label %937, label %947

937:                                              ; preds = %935
  %938 = icmp slt i32 %936, 1
  %939 = and i1 %877, %938
  br i1 %939, label %940, label %945

940:                                              ; preds = %937
  %941 = icmp ne i32 %936, 0
  %942 = and i128 %782, 1
  %943 = icmp eq i128 %942, 0
  %944 = select i1 %941, i1 true, i1 %943
  br i1 %944, label %947, label %945

945:                                              ; preds = %940, %937
  %946 = add i128 %783, 1
  br label %947

947:                                              ; preds = %945, %940, %935
  %948 = phi i128 [ %946, %945 ], [ %783, %940 ], [ %783, %935 ]
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %10) #10
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %10, i8 0, i64 5604, i1 false), !alias.scope !118
  %949 = icmp eq i128 %948, 0
  br i1 %949, label %959, label %950

950:                                              ; preds = %947, %950
  %951 = phi i128 [ %957, %950 ], [ %948, %947 ]
  %952 = trunc i128 %951 to i32
  %953 = load i32, ptr %10, align 4, !tbaa !34, !alias.scope !118
  %954 = add nsw i32 %953, 1
  store i32 %954, ptr %10, align 4, !tbaa !34, !alias.scope !118
  %955 = sext i32 %953 to i64
  %956 = getelementptr inbounds [1400 x i32], ptr %703, i64 0, i64 %955
  store i32 %952, ptr %956, align 4, !tbaa !24, !alias.scope !118
  %957 = lshr i128 %951, 32
  %958 = icmp ult i128 %951, 4294967296
  br i1 %958, label %959, label %950
959:                                              ; preds = %950, %947
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %13, ptr noundef nonnull align 4 dereferenceable(5604) %10, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %10) #10
  %960 = sub i32 %704, %708
  br label %1041

961:                                              ; preds = %882
  br i1 %885, label %962, label %964

962:                                              ; preds = %961
  %963 = zext i32 %712 to i64
  br label %967

964:                                              ; preds = %967, %961
  %965 = phi i64 [ 0, %961 ], [ %976, %967 ]
  %966 = icmp eq i64 %965, 0
  br i1 %966, label %987, label %979

967:                                              ; preds = %967, %962
  %968 = phi i64 [ 0, %962 ], [ %977, %967 ]
  %969 = phi i64 [ 0, %962 ], [ %976, %967 ]
  %970 = getelementptr inbounds [1400 x i32], ptr %700, i64 0, i64 %968
  %971 = load i32, ptr %970, align 4, !tbaa !24
  %972 = zext i32 %971 to i64
  %973 = mul nuw nsw i64 %972, 10
  %974 = add nuw nsw i64 %973, %969
  %975 = trunc i64 %974 to i32
  store i32 %975, ptr %970, align 4, !tbaa !24
  %976 = lshr i64 %974, 32
  %977 = add nuw nsw i64 %968, 1
  %978 = icmp eq i64 %977, %963
  br i1 %978, label %964, label %967
979:                                              ; preds = %964
  %980 = icmp eq i32 %712, 1400
  br i1 %980, label %981, label %982

981:                                              ; preds = %979
  store i32 %707, ptr %230, align 4
  tail call void @llvm.trap()
  unreachable

982:                                              ; preds = %979
  %983 = trunc i64 %965 to i32
  %984 = add nsw i32 %712, 1
  store i32 %984, ptr %5, align 4, !tbaa !34
  %985 = sext i32 %712 to i64
  %986 = getelementptr inbounds [1400 x i32], ptr %700, i64 0, i64 %985
  store i32 %983, ptr %986, align 4, !tbaa !24
  br label %987

987:                                              ; preds = %982, %964
  %988 = icmp sgt i32 %784, 0
  br i1 %988, label %989, label %991

989:                                              ; preds = %987
  %990 = zext i32 %784 to i64
  br label %994

991:                                              ; preds = %994, %987
  %992 = phi i64 [ 0, %987 ], [ %1003, %994 ]
  %993 = icmp eq i64 %992, 0
  br i1 %993, label %1014, label %1006

994:                                              ; preds = %994, %989
  %995 = phi i64 [ 0, %989 ], [ %1004, %994 ]
  %996 = phi i64 [ 0, %989 ], [ %1003, %994 ]
  %997 = getelementptr inbounds [1400 x i32], ptr %235, i64 0, i64 %995
  %998 = load i32, ptr %997, align 4, !tbaa !24
  %999 = zext i32 %998 to i64
  %1000 = mul nuw nsw i64 %999, 10
  %1001 = add nuw nsw i64 %1000, %996
  %1002 = trunc i64 %1001 to i32
  store i32 %1002, ptr %997, align 4, !tbaa !24
  %1003 = lshr i64 %1001, 32
  %1004 = add nuw nsw i64 %995, 1
  %1005 = icmp eq i64 %1004, %990
  br i1 %1005, label %991, label %994
1006:                                             ; preds = %991
  %1007 = icmp eq i32 %784, 1400
  br i1 %1007, label %1008, label %1009

1008:                                             ; preds = %1006
  store i32 %707, ptr %230, align 4
  tail call void @llvm.trap()
  unreachable

1009:                                             ; preds = %1006
  %1010 = trunc i64 %992 to i32
  %1011 = add nsw i32 %784, 1
  store i32 %1011, ptr %7, align 4, !tbaa !34
  %1012 = sext i32 %784 to i64
  %1013 = getelementptr inbounds [1400 x i32], ptr %235, i64 0, i64 %1012
  store i32 %1010, ptr %1013, align 4, !tbaa !24
  br label %1014

1014:                                             ; preds = %1009, %991
  %1015 = icmp sgt i32 %848, 0
  br i1 %1015, label %1016, label %1018

1016:                                             ; preds = %1014
  %1017 = zext i32 %848 to i64
  br label %1021

1018:                                             ; preds = %1021, %1014
  %1019 = phi i64 [ 0, %1014 ], [ %1030, %1021 ]
  %1020 = icmp eq i64 %1019, 0
  br i1 %1020, label %1041, label %1033

1021:                                             ; preds = %1021, %1016
  %1022 = phi i64 [ 0, %1016 ], [ %1031, %1021 ]
  %1023 = phi i64 [ 0, %1016 ], [ %1030, %1021 ]
  %1024 = getelementptr inbounds [1400 x i32], ptr %236, i64 0, i64 %1022
  %1025 = load i32, ptr %1024, align 4, !tbaa !24
  %1026 = zext i32 %1025 to i64
  %1027 = mul nuw nsw i64 %1026, 10
  %1028 = add nuw nsw i64 %1027, %1023
  %1029 = trunc i64 %1028 to i32
  store i32 %1029, ptr %1024, align 4, !tbaa !24
  %1030 = lshr i64 %1028, 32
  %1031 = add nuw nsw i64 %1022, 1
  %1032 = icmp eq i64 %1031, %1017
  br i1 %1032, label %1018, label %1021
1033:                                             ; preds = %1018
  %1034 = icmp eq i32 %848, 1400
  br i1 %1034, label %1035, label %1036

1035:                                             ; preds = %1033
  store i32 %707, ptr %230, align 4
  tail call void @llvm.trap()
  unreachable

1036:                                             ; preds = %1033
  %1037 = trunc i64 %1019 to i32
  %1038 = add nsw i32 %848, 1
  store i32 %1038, ptr %8, align 4, !tbaa !34
  %1039 = sext i32 %848 to i64
  %1040 = getelementptr inbounds [1400 x i32], ptr %236, i64 0, i64 %1039
  store i32 %1037, ptr %1040, align 4, !tbaa !24
  br label %1041

1041:                                             ; preds = %1036, %1018, %959
  %1042 = phi i32 [ %960, %959 ], [ %707, %1018 ], [ %707, %1036 ]
  %1043 = phi i128 [ %948, %959 ], [ %783, %1018 ], [ %783, %1036 ]
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %9) #10
  %1044 = add nuw nsw i32 %708, 1
  br i1 %884, label %1045, label %706
1045:                                             ; preds = %1041
  store i32 %1042, ptr %230, align 4
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %8) #10
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %7) #10
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %6) #10
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %5) #10
  br label %1046

1046:                                             ; preds = %1045, %192
  call void @llvm.lifetime.start.p0(i64 48, ptr nonnull %14) #10
  %1047 = getelementptr inbounds i8, ptr %13, i64 4
  br label %1048

1048:                                             ; preds = %1086, %1046
  %1049 = phi i32 [ %1093, %1086 ], [ -1, %1046 ]
  %1050 = phi i32 [ %1092, %1086 ], [ -1, %1046 ]
  %1051 = phi i32 [ %1091, %1086 ], [ 0, %1046 ]
  %1052 = phi i64 [ %1087, %1086 ], [ 0, %1046 ]
  %1053 = load i32, ptr %13, align 4, !tbaa !34
  %1054 = icmp eq i32 %1053, 0
  br i1 %1054, label %1060, label %1055

1055:                                             ; preds = %1048
  %1056 = sext i32 %1053 to i64
  br label %1073

1057:                                             ; preds = %1073
  %1058 = trunc i64 %1084 to i8
  %1059 = or i8 %1058, 48
  br label %1060

1060:                                             ; preds = %1057, %1048
  %1061 = phi i8 [ 48, %1048 ], [ %1059, %1057 ]
  br i1 %1054, label %1086, label %1062

1062:                                             ; preds = %1060
  %1063 = sext i32 %1053 to i64
  br label %1064

1064:                                             ; preds = %1070, %1062
  %1065 = phi i64 [ %1063, %1062 ], [ %1066, %1070 ]
  %1066 = add nsw i64 %1065, -1
  %1067 = getelementptr inbounds [1400 x i32], ptr %1047, i64 0, i64 %1066
  %1068 = load i32, ptr %1067, align 4, !tbaa !24
  %1069 = icmp eq i32 %1068, 0
  br i1 %1069, label %1070, label %1086

1070:                                             ; preds = %1064
  %1071 = trunc i64 %1066 to i32
  store i32 %1071, ptr %13, align 4, !tbaa !34
  %1072 = icmp eq i64 %1066, 0
  br i1 %1072, label %1086, label %1064
1073:                                             ; preds = %1073, %1055
  %1074 = phi i64 [ %1056, %1055 ], [ %1076, %1073 ]
  %1075 = phi i64 [ 0, %1055 ], [ %1084, %1073 ]
  %1076 = add nsw i64 %1074, -1
  %1077 = shl nuw nsw i64 %1075, 32
  %1078 = getelementptr inbounds [1400 x i32], ptr %1047, i64 0, i64 %1076
  %1079 = load i32, ptr %1078, align 4, !tbaa !24
  %1080 = zext i32 %1079 to i64
  %1081 = or i64 %1077, %1080
  %1082 = udiv i64 %1081, 10
  %1083 = trunc i64 %1082 to i32
  store i32 %1083, ptr %1078, align 4, !tbaa !24
  %1084 = urem i64 %1081, 10
  %1085 = icmp eq i64 %1076, 0
  br i1 %1085, label %1057, label %1073
1086:                                             ; preds = %1064, %1070, %1060
  %1087 = add nuw nsw i64 %1052, 1
  %1088 = getelementptr inbounds [48 x i8], ptr %14, i64 0, i64 %1052
  store i8 %1061, ptr %1088, align 1, !tbaa !26
  %1089 = load i32, ptr %13, align 4, !tbaa !25
  %1090 = icmp eq i32 %1089, 0
  %1091 = add nuw i32 %1051, 1
  %1092 = add i32 %1050, 1
  %1093 = add i32 %1049, -1
  br i1 %1090, label %1094, label %1048
1094:                                             ; preds = %1086
  %1095 = trunc i64 %1052 to i32
  %1096 = trunc i64 %1087 to i32
  %1097 = icmp eq i64 %1052, 0
  br i1 %1097, label %1118, label %1098

1098:                                             ; preds = %1094
  %1099 = getelementptr inbounds i8, ptr %13, i64 5604
  %1100 = load i32, ptr %1099, align 4
  %1101 = add i32 %1100, %1051
  %1102 = zext i32 %1051 to i64
  br label %1103

1103:                                             ; preds = %1098, %1109
  %1104 = phi i64 [ 0, %1098 ], [ %1111, %1109 ]
  %1105 = phi i32 [ %1100, %1098 ], [ %1110, %1109 ]
  %1106 = getelementptr inbounds [48 x i8], ptr %14, i64 0, i64 %1104
  %1107 = load i8, ptr %1106, align 1, !tbaa !26
  %1108 = icmp eq i8 %1107, 48
  br i1 %1108, label %1109, label %1113

1109:                                             ; preds = %1103
  %1110 = add nsw i32 %1105, 1
  %1111 = add nuw nsw i64 %1104, 1
  %1112 = icmp eq i64 %1111, %1102
  br i1 %1112, label %1115, label %1103
1113:                                             ; preds = %1103
  %1114 = trunc i64 %1104 to i32
  br label %1115

1115:                                             ; preds = %1109, %1113
  %1116 = phi i32 [ %1105, %1113 ], [ %1101, %1109 ]
  %1117 = phi i32 [ %1114, %1113 ], [ %1092, %1109 ]
  store i32 %1116, ptr %1099, align 4
  br label %1118

1118:                                             ; preds = %1115, %1094
  %1119 = phi i32 [ 0, %1094 ], [ %1117, %1115 ]
  %1120 = sub nsw i32 %1096, %1119
  %1121 = getelementptr inbounds i8, ptr %13, i64 5604
  %1122 = load i32, ptr %1121, align 4, !tbaa !35
  %1123 = add nsw i32 %1122, %1120
  %1124 = add nsw i32 %1123, -1
  %1125 = icmp slt i32 %1123, -5
  %1126 = icmp sgt i32 %1122, 6
  %1127 = or i1 %1126, %1125
  br i1 %1127, label %1188, label %1128

1128:                                             ; preds = %1118
  %1129 = icmp slt i32 %1123, 1
  br i1 %1129, label %1130, label %1150

1130:                                             ; preds = %1128
  %1131 = zext i32 %170 to i64
  %1132 = getelementptr inbounds i8, ptr %0, i64 %1131
  store i8 48, ptr %1132, align 1, !tbaa !26
  %1133 = or i32 %170, 2
  %1134 = getelementptr inbounds i8, ptr %1132, i64 1
  store i8 46, ptr %1134, align 1, !tbaa !26
  %1135 = icmp slt i32 %1123, 0
  br i1 %1135, label %1136, label %1150

1136:                                             ; preds = %1130
  %1137 = zext i32 %1133 to i64
  %1138 = sub i32 %1119, %1122
  %1139 = add i32 %1138, %1049
  %1140 = tail call i32 @llvm.smax.i32(i32 %1139, i32 1)
  %1141 = add nuw i32 %1133, %1140
  %1142 = zext i32 %1141 to i64
  br label %1143

1143:                                             ; preds = %1136, %1143
  %1144 = phi i64 [ %1137, %1136 ], [ %1145, %1143 ]
  %1145 = add nuw nsw i64 %1144, 1
  %1146 = getelementptr inbounds i8, ptr %0, i64 %1144
  store i8 48, ptr %1146, align 1, !tbaa !26
  %1147 = icmp eq i64 %1145, %1142
  br i1 %1147, label %1148, label %1143
1148:                                             ; preds = %1143
  %1149 = trunc i64 %1145 to i32
  br label %1150

1150:                                             ; preds = %1148, %1130, %1128
  %1151 = phi i32 [ %170, %1128 ], [ %1133, %1130 ], [ %1149, %1148 ]
  %1152 = icmp sgt i32 %1119, %1095
  br i1 %1152, label %1155, label %1153

1153:                                             ; preds = %1150
  %1154 = sext i32 %1119 to i64
  br label %1161

1155:                                             ; preds = %1178, %1150
  %1156 = phi i32 [ %1151, %1150 ], [ %1179, %1178 ]
  %1157 = phi i32 [ %1123, %1150 ], [ %1122, %1178 ]
  %1158 = icmp sgt i32 %1157, 0
  br i1 %1158, label %1159, label %1256

1159:                                             ; preds = %1155
  %1160 = sext i32 %1156 to i64
  br label %1181

1161:                                             ; preds = %1153, %1178
  %1162 = phi i64 [ %1052, %1153 ], [ %1180, %1178 ]
  %1163 = phi i32 [ %1123, %1153 ], [ %1170, %1178 ]
  %1164 = phi i32 [ %1151, %1153 ], [ %1179, %1178 ]
  %1165 = getelementptr inbounds [48 x i8], ptr %14, i64 0, i64 %1162
  %1166 = load i8, ptr %1165, align 1, !tbaa !26
  %1167 = add nsw i32 %1164, 1
  %1168 = sext i32 %1164 to i64
  %1169 = getelementptr inbounds i8, ptr %0, i64 %1168
  store i8 %1166, ptr %1169, align 1, !tbaa !26
  %1170 = add nsw i32 %1163, -1
  %1171 = icmp eq i32 %1170, 0
  %1172 = icmp sgt i64 %1162, %1154
  %1173 = and i1 %1171, %1172
  br i1 %1173, label %1174, label %1178

1174:                                             ; preds = %1161
  %1175 = add nsw i32 %1164, 2
  %1176 = sext i32 %1167 to i64
  %1177 = getelementptr inbounds i8, ptr %0, i64 %1176
  store i8 46, ptr %1177, align 1, !tbaa !26
  br label %1178

1178:                                             ; preds = %1161, %1174
  %1179 = phi i32 [ %1175, %1174 ], [ %1167, %1161 ]
  %1180 = add nsw i64 %1162, -1
  br i1 %1172, label %1161, label %1155
1181:                                             ; preds = %1159, %1181
  %1182 = phi i64 [ %1160, %1159 ], [ %1185, %1181 ]
  %1183 = phi i32 [ %1157, %1159 ], [ %1184, %1181 ]
  %1184 = add nsw i32 %1183, -1
  %1185 = add nsw i64 %1182, 1
  %1186 = getelementptr inbounds i8, ptr %0, i64 %1182
  store i8 48, ptr %1186, align 1, !tbaa !26
  %1187 = icmp sgt i32 %1183, 1
  br i1 %1187, label %1181, label %1254
1188:                                             ; preds = %1118
  %1189 = add nuw nsw i32 %170, 1
  %1190 = zext i32 %170 to i64
  %1191 = getelementptr inbounds i8, ptr %0, i64 %1190
  store i8 %1061, ptr %1191, align 1, !tbaa !26
  %1192 = icmp sgt i32 %1120, 1
  br i1 %1192, label %1193, label %1197

1193:                                             ; preds = %1188
  %1194 = or i32 %170, 2
  %1195 = zext i32 %1189 to i64
  %1196 = getelementptr inbounds i8, ptr %0, i64 %1195
  store i8 46, ptr %1196, align 1, !tbaa !26
  br label %1197

1197:                                             ; preds = %1193, %1188
  %1198 = phi i32 [ %1194, %1193 ], [ %1189, %1188 ]
  %1199 = icmp slt i32 %1119, %1095
  br i1 %1199, label %1200, label %1207

1200:                                             ; preds = %1197
  %1201 = zext i32 %1198 to i64
  %1202 = sub i32 %1198, %1119
  %1203 = add i32 %1202, %1051
  %1204 = zext i32 %1203 to i64
  br label %1245

1205:                                             ; preds = %1245
  %1206 = trunc i64 %1251 to i32
  br label %1207

1207:                                             ; preds = %1205, %1197
  %1208 = phi i32 [ %1198, %1197 ], [ %1206, %1205 ]
  %1209 = sext i32 %1208 to i64
  %1210 = getelementptr inbounds i8, ptr %0, i64 %1209
  store i8 101, ptr %1210, align 1, !tbaa !26
  %1211 = icmp slt i32 %1123, 1
  %1212 = select i1 %1211, i8 45, i8 43
  %1213 = getelementptr i8, ptr %1210, i64 1
  store i8 %1212, ptr %1213, align 1, !tbaa !26
  %1214 = sub nsw i32 1, %1123
  %1215 = select i1 %1211, i32 %1214, i32 %1124
  call void @llvm.lifetime.start.p0(i64 12, ptr nonnull %4) #10
  br label %1216

1216:                                             ; preds = %1216, %1207
  %1217 = phi i32 [ %1227, %1216 ], [ 1, %1207 ]
  %1218 = phi i64 [ %1223, %1216 ], [ 0, %1207 ]
  %1219 = phi i32 [ %1225, %1216 ], [ %1215, %1207 ]
  %1220 = urem i32 %1219, 10
  %1221 = trunc i32 %1220 to i8
  %1222 = or i8 %1221, 48
  %1223 = add nuw nsw i64 %1218, 1
  %1224 = getelementptr inbounds [12 x i8], ptr %4, i64 0, i64 %1218
  store i8 %1222, ptr %1224, align 1, !tbaa !26
  %1225 = udiv i32 %1219, 10
  %1226 = icmp ult i32 %1219, 10
  %1227 = add nuw i32 %1217, 1
  br i1 %1226, label %1228, label %1216
1228:                                             ; preds = %1216
  %1229 = add nsw i32 %1208, 2
  %1230 = sext i32 %1229 to i64
  %1231 = getelementptr inbounds i8, ptr %0, i64 %1230
  %1232 = and i64 %1218, 4294967295
  %1233 = zext i32 %1217 to i64
  br label %1234

1234:                                             ; preds = %1234, %1228
  %1235 = phi i64 [ 0, %1228 ], [ %1240, %1234 ]
  %1236 = sub nuw nsw i64 %1232, %1235
  %1237 = getelementptr inbounds [12 x i8], ptr %4, i64 0, i64 %1236
  %1238 = load i8, ptr %1237, align 1, !tbaa !26
  %1239 = getelementptr inbounds i8, ptr %1231, i64 %1235
  store i8 %1238, ptr %1239, align 1, !tbaa !26
  %1240 = add nuw nsw i64 %1235, 1
  %1241 = icmp eq i64 %1240, %1233
  br i1 %1241, label %1242, label %1234
1242:                                             ; preds = %1234
  %1243 = trunc i64 %1223 to i32
  call void @llvm.lifetime.end.p0(i64 12, ptr nonnull %4) #10
  %1244 = add nsw i32 %1229, %1243
  br label %1256

1245:                                             ; preds = %1200, %1245
  %1246 = phi i64 [ %1201, %1200 ], [ %1251, %1245 ]
  %1247 = phi i64 [ %1052, %1200 ], [ %1248, %1245 ]
  %1248 = add nsw i64 %1247, -1
  %1249 = getelementptr inbounds [48 x i8], ptr %14, i64 0, i64 %1248
  %1250 = load i8, ptr %1249, align 1, !tbaa !26
  %1251 = add nuw nsw i64 %1246, 1
  %1252 = getelementptr inbounds i8, ptr %0, i64 %1246
  store i8 %1250, ptr %1252, align 1, !tbaa !26
  %1253 = icmp eq i64 %1251, %1204
  br i1 %1253, label %1205, label %1245
1254:                                             ; preds = %1181
  %1255 = trunc i64 %1185 to i32
  br label %1256

1256:                                             ; preds = %1254, %1155, %1242
  %1257 = phi i32 [ %1244, %1242 ], [ %1156, %1155 ], [ %1255, %1254 ]
  call void @llvm.lifetime.end.p0(i64 48, ptr nonnull %14) #10
  br label %1260

1258:                                             ; preds = %176
  %1259 = trunc i64 %181 to i32
  br label %1260

1260:                                             ; preds = %1258, %1256, %188
  %1261 = phi i32 [ %1257, %1256 ], [ %189, %188 ], [ %1259, %1258 ]
  call void @llvm.lifetime.end.p0(i64 5616, ptr nonnull %13) #10
  br label %1262

1262:                                             ; preds = %1260, %158
  %1263 = phi i32 [ %159, %158 ], [ %1261, %1260 ]
  call void @llvm.lifetime.end.p0(i64 36, ptr nonnull %11) #10
  ret i32 %1263
}

; Function Attrs: nounwind
define weak hidden i32 @tz_soft_parse(ptr noundef writeonly %0, ptr noundef readonly %1, i64 noundef %2, i32 noundef %3) local_unnamed_addr #4 {
  %5 = alloca %struct.tzrt_format, align 8
  %6 = alloca i64, align 8
  %7 = alloca %struct.tzrt_big, align 4
  %8 = alloca %struct.tzrt_big, align 4
  %9 = alloca i32, align 4
  %10 = add i64 %2, -4097
  %11 = icmp ult i64 %10, -4096
  br i1 %11, label %442, label %12

12:                                               ; preds = %4
  call void @llvm.lifetime.start.p0(i64 36, ptr nonnull %5) #10
  switch i32 %3, label %76 [
    i32 0, label %13
    i32 1, label %22
    i32 2, label %31
    i32 3, label %40
    i32 4, label %49
    i32 5, label %58
    i32 6, label %67
  ]

13:                                               ; preds = %12
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !130
  %14 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 11, ptr %14, align 4, !tbaa !12, !alias.scope !130
  %15 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -24, ptr %15, align 8, !tbaa !13, !alias.scope !130
  %16 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 5, ptr %16, align 4, !tbaa !14, !alias.scope !130
  %17 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 10, ptr %17, align 8, !tbaa !15, !alias.scope !130
  %18 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 15, ptr %18, align 4, !tbaa !16, !alias.scope !130
  %19 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 16, ptr %19, align 8, !tbaa !17, !alias.scope !130
  %20 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %20, align 4, !tbaa !18, !alias.scope !130
  %21 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %21, align 8, !tbaa !19, !alias.scope !130
  br label %89

22:                                               ; preds = %12
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !130
  %23 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 24, ptr %23, align 4, !tbaa !12, !alias.scope !130
  %24 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -149, ptr %24, align 8, !tbaa !13, !alias.scope !130
  %25 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 104, ptr %25, align 4, !tbaa !14, !alias.scope !130
  %26 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 23, ptr %26, align 8, !tbaa !15, !alias.scope !130
  %27 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 127, ptr %27, align 4, !tbaa !16, !alias.scope !130
  %28 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 32, ptr %28, align 8, !tbaa !17, !alias.scope !130
  %29 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %29, align 4, !tbaa !18, !alias.scope !130
  %30 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %30, align 8, !tbaa !19, !alias.scope !130
  br label %89

31:                                               ; preds = %12
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !130
  %32 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 53, ptr %32, align 4, !tbaa !12, !alias.scope !130
  %33 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -1074, ptr %33, align 8, !tbaa !13, !alias.scope !130
  %34 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 971, ptr %34, align 4, !tbaa !14, !alias.scope !130
  %35 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 52, ptr %35, align 8, !tbaa !15, !alias.scope !130
  %36 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 1023, ptr %36, align 4, !tbaa !16, !alias.scope !130
  %37 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 64, ptr %37, align 8, !tbaa !17, !alias.scope !130
  %38 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %38, align 4, !tbaa !18, !alias.scope !130
  %39 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %39, align 8, !tbaa !19, !alias.scope !130
  br label %89

40:                                               ; preds = %12
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !130
  %41 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 113, ptr %41, align 4, !tbaa !12, !alias.scope !130
  %42 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -16494, ptr %42, align 8, !tbaa !13, !alias.scope !130
  %43 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 16271, ptr %43, align 4, !tbaa !14, !alias.scope !130
  %44 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 112, ptr %44, align 8, !tbaa !15, !alias.scope !130
  %45 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 16383, ptr %45, align 4, !tbaa !16, !alias.scope !130
  %46 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 128, ptr %46, align 8, !tbaa !17, !alias.scope !130
  %47 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %47, align 4, !tbaa !18, !alias.scope !130
  %48 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %48, align 8, !tbaa !19, !alias.scope !130
  br label %89

49:                                               ; preds = %12
  store i32 10, ptr %5, align 8, !tbaa !4, !alias.scope !130
  %50 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 7, ptr %50, align 4, !tbaa !12, !alias.scope !130
  %51 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -101, ptr %51, align 8, !tbaa !13, !alias.scope !130
  %52 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 90, ptr %52, align 4, !tbaa !14, !alias.scope !130
  %53 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 23, ptr %53, align 8, !tbaa !15, !alias.scope !130
  %54 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 101, ptr %54, align 4, !tbaa !16, !alias.scope !130
  %55 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 32, ptr %55, align 8, !tbaa !17, !alias.scope !130
  %56 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %56, align 4, !tbaa !18, !alias.scope !130
  %57 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %57, align 8, !tbaa !19, !alias.scope !130
  br label %89

58:                                               ; preds = %12
  store i32 10, ptr %5, align 8, !tbaa !4, !alias.scope !130
  %59 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 16, ptr %59, align 4, !tbaa !12, !alias.scope !130
  %60 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -398, ptr %60, align 8, !tbaa !13, !alias.scope !130
  %61 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 369, ptr %61, align 4, !tbaa !14, !alias.scope !130
  %62 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 53, ptr %62, align 8, !tbaa !15, !alias.scope !130
  %63 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 398, ptr %63, align 4, !tbaa !16, !alias.scope !130
  %64 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 64, ptr %64, align 8, !tbaa !17, !alias.scope !130
  %65 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %65, align 4, !tbaa !18, !alias.scope !130
  %66 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %66, align 8, !tbaa !19, !alias.scope !130
  br label %89

67:                                               ; preds = %12
  store i32 10, ptr %5, align 8, !tbaa !4, !alias.scope !130
  %68 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 34, ptr %68, align 4, !tbaa !12, !alias.scope !130
  %69 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -6176, ptr %69, align 8, !tbaa !13, !alias.scope !130
  %70 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 6111, ptr %70, align 4, !tbaa !14, !alias.scope !130
  %71 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 113, ptr %71, align 8, !tbaa !15, !alias.scope !130
  %72 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 6176, ptr %72, align 4, !tbaa !16, !alias.scope !130
  %73 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 128, ptr %73, align 8, !tbaa !17, !alias.scope !130
  %74 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %74, align 4, !tbaa !18, !alias.scope !130
  %75 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %75, align 8, !tbaa !19, !alias.scope !130
  br label %89

76:                                               ; preds = %12
  %77 = and i32 %3, 7
  %78 = shl nuw nsw i32 8, %77
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !130
  %79 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 %78, ptr %79, align 4, !tbaa !12, !alias.scope !130
  %80 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 0, ptr %80, align 8, !tbaa !13, !alias.scope !130
  %81 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 0, ptr %81, align 4, !tbaa !14, !alias.scope !130
  %82 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 0, ptr %82, align 8, !tbaa !15, !alias.scope !130
  %83 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 0, ptr %83, align 4, !tbaa !16, !alias.scope !130
  %84 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 %78, ptr %84, align 8, !tbaa !17, !alias.scope !130
  %85 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 1, ptr %85, align 4, !tbaa !18, !alias.scope !130
  %86 = getelementptr inbounds i8, ptr %5, i64 32
  %87 = icmp slt i32 %3, 24
  %88 = zext i1 %87 to i32
  store i32 %88, ptr %86, align 8, !tbaa !19, !alias.scope !130
  br label %89

89:                                               ; preds = %13, %22, %31, %40, %49, %58, %67, %76
  %90 = phi i32 [ 1, %13 ], [ 1, %22 ], [ 1, %31 ], [ 1, %40 ], [ 1, %49 ], [ 1, %58 ], [ 1, %67 ], [ %88, %76 ]
  %91 = phi i1 [ true, %13 ], [ true, %22 ], [ true, %31 ], [ true, %40 ], [ true, %49 ], [ true, %58 ], [ true, %67 ], [ false, %76 ]
  %92 = phi i32 [ 16, %13 ], [ 32, %22 ], [ 64, %31 ], [ 128, %40 ], [ 32, %49 ], [ 64, %58 ], [ 128, %67 ], [ %78, %76 ]
  %93 = phi i32 [ 31, %13 ], [ 255, %22 ], [ 2047, %31 ], [ 32767, %40 ], [ 203, %49 ], [ 797, %58 ], [ 12353, %67 ], [ 1, %76 ]
  %94 = phi i32 [ 10, %13 ], [ 23, %22 ], [ 52, %31 ], [ 112, %40 ], [ 23, %49 ], [ 53, %58 ], [ 113, %67 ], [ 0, %76 ]
  %95 = phi i32 [ 5, %13 ], [ 104, %22 ], [ 971, %31 ], [ 16271, %40 ], [ 90, %49 ], [ 369, %58 ], [ 6111, %67 ], [ 0, %76 ]
  %96 = phi i32 [ -24, %13 ], [ -149, %22 ], [ -1074, %31 ], [ -16494, %40 ], [ -101, %49 ], [ -398, %58 ], [ -6176, %67 ], [ 0, %76 ]
  %97 = phi i32 [ 11, %13 ], [ 24, %22 ], [ 53, %31 ], [ 113, %40 ], [ 7, %49 ], [ 16, %58 ], [ 34, %67 ], [ %78, %76 ]
  %98 = phi i1 [ true, %13 ], [ true, %22 ], [ true, %31 ], [ true, %40 ], [ false, %49 ], [ false, %58 ], [ false, %67 ], [ true, %76 ]
  %99 = phi i1 [ false, %13 ], [ false, %22 ], [ false, %31 ], [ false, %40 ], [ true, %49 ], [ true, %58 ], [ true, %67 ], [ false, %76 ]
  call void @llvm.lifetime.start.p0(i64 8, ptr nonnull %6) #10
  store i64 0, ptr %6, align 8, !tbaa !133
  %100 = load i8, ptr %1, align 1, !tbaa !26
  %101 = icmp eq i8 %100, 45
  %102 = zext i1 %101 to i32
  switch i8 %100, label %104 [
    i8 45, label %103
    i8 43, label %103
  ]

103:                                              ; preds = %89, %89
  store i64 1, ptr %6, align 8, !tbaa !133
  br label %104

104:                                              ; preds = %89, %103
  %105 = load i64, ptr %6, align 8, !tbaa !133
  %106 = icmp eq i64 %105, %2
  br i1 %106, label %440, label %107

107:                                              ; preds = %104
  br i1 %91, label %212, label %108

108:                                              ; preds = %107
  %109 = icmp eq i32 %90, 0
  %110 = and i1 %109, %101
  br i1 %110, label %440, label %111

111:                                              ; preds = %108
  %112 = add i64 %105, 1
  %113 = icmp ult i64 %112, %2
  br i1 %113, label %114, label %125

114:                                              ; preds = %111
  %115 = getelementptr inbounds i8, ptr %1, i64 %105
  %116 = load i8, ptr %115, align 1, !tbaa !26
  %117 = icmp eq i8 %116, 48
  br i1 %117, label %118, label %125

118:                                              ; preds = %114
  %119 = getelementptr inbounds i8, ptr %1, i64 %112
  %120 = load i8, ptr %119, align 1, !tbaa !26
  switch i8 %120, label %125 [
    i8 120, label %122
    i8 98, label %121
  ]

121:                                              ; preds = %118
  br label %122

122:                                              ; preds = %118, %121
  %123 = phi i32 [ 2, %121 ], [ 16, %118 ]
  %124 = add i64 %105, 2
  store i64 %124, ptr %6, align 8, !tbaa !133
  br label %125

125:                                              ; preds = %122, %118, %114, %111
  %126 = phi i32 [ 10, %114 ], [ 10, %111 ], [ 10, %118 ], [ %123, %122 ]
  %127 = load i64, ptr %6, align 8, !tbaa !133
  %128 = icmp eq i64 %127, %2
  br i1 %128, label %440, label %129

129:                                              ; preds = %125
  %130 = sub nuw nsw i32 %92, %90
  %131 = icmp eq i32 %130, 128
  %132 = zext i32 %130 to i128
  %133 = shl nsw i128 -1, %132
  %134 = xor i128 %133, -1
  %135 = select i1 %131, i128 -1, i128 %134
  %136 = icmp ne i32 %90, 0
  %137 = and i1 %136, %101
  %138 = zext i1 %137 to i128
  %139 = add i128 %135, %138
  %140 = zext i32 %126 to i128
  %141 = freeze i128 %139
  %142 = udiv i128 %141, %140
  %143 = mul i128 %142, %140
  %144 = sub i128 %141, %143
  %145 = trunc i128 %144 to i32
  %146 = icmp ult i64 %127, %2
  br i1 %146, label %147, label %197

147:                                              ; preds = %129, %192
  %148 = phi i128 [ %194, %192 ], [ 0, %129 ]
  %149 = phi i32 [ %193, %192 ], [ 0, %129 ]
  %150 = phi i64 [ %195, %192 ], [ %127, %129 ]
  %151 = getelementptr inbounds i8, ptr %1, i64 %150
  %152 = load i8, ptr %151, align 1, !tbaa !26
  %153 = icmp eq i8 %152, 95
  br i1 %153, label %154, label %159

154:                                              ; preds = %147
  %155 = icmp eq i32 %149, 0
  %156 = add nuw nsw i64 %150, 1
  %157 = icmp eq i64 %156, %2
  %158 = select i1 %155, i1 true, i1 %157
  br i1 %158, label %439, label %192

159:                                              ; preds = %147
  %160 = sext i8 %152 to i32
  %161 = add i8 %152, -48
  %162 = icmp ult i8 %161, 10
  br i1 %162, label %163, label %165

163:                                              ; preds = %159
  %164 = add nsw i32 %160, -48
  br label %175

165:                                              ; preds = %159
  %166 = add i8 %152, -97
  %167 = icmp ult i8 %166, 6
  br i1 %167, label %168, label %170

168:                                              ; preds = %165
  %169 = add nsw i32 %160, -87
  br label %175

170:                                              ; preds = %165
  %171 = add i8 %152, -65
  %172 = icmp ult i8 %171, 6
  %173 = add nsw i32 %160, -55
  %174 = select i1 %172, i32 %173, i32 -1
  br label %175

175:                                              ; preds = %163, %168, %170
  %176 = phi i32 [ %164, %163 ], [ %169, %168 ], [ %174, %170 ]
  %177 = icmp uge i32 %176, %126
  %178 = icmp ugt i128 %148, %142
  %179 = select i1 %177, i1 true, i1 %178
  br i1 %179, label %188, label %180

180:                                              ; preds = %175
  %181 = icmp eq i128 %148, %142
  %182 = icmp ugt i32 %176, %145
  %183 = select i1 %181, i1 %182, i1 false
  br i1 %183, label %188, label %184

184:                                              ; preds = %180
  %185 = mul i128 %148, %140
  %186 = zext i32 %176 to i128
  %187 = add i128 %185, %186
  br label %188

188:                                              ; preds = %175, %180, %184
  %189 = phi i32 [ 1, %184 ], [ %149, %180 ], [ %149, %175 ]
  %190 = phi i128 [ %187, %184 ], [ %148, %180 ], [ %148, %175 ]
  %191 = phi i1 [ true, %184 ], [ false, %180 ], [ false, %175 ]
  br i1 %191, label %192, label %439

192:                                              ; preds = %154, %188
  %193 = phi i32 [ %189, %188 ], [ 0, %154 ]
  %194 = phi i128 [ %190, %188 ], [ %148, %154 ]
  %195 = add nuw i64 %150, 1
  %196 = icmp eq i64 %195, %2
  br i1 %196, label %197, label %147
197:                                              ; preds = %192, %129
  %198 = phi i64 [ %127, %129 ], [ %2, %192 ]
  %199 = phi i128 [ 0, %129 ], [ %194, %192 ]
  store i64 %198, ptr %6, align 8
  %200 = sub i128 0, %199
  %201 = select i1 %101, i128 %200, i128 %199
  %202 = lshr exact i32 %92, 3
  %203 = zext i32 %202 to i64
  br label %204

204:                                              ; preds = %204, %197
  %205 = phi i64 [ 0, %197 ], [ %210, %204 ]
  %206 = phi i128 [ %201, %197 ], [ %209, %204 ]
  %207 = trunc i128 %206 to i8
  %208 = getelementptr inbounds i8, ptr %0, i64 %205
  store i8 %207, ptr %208, align 1, !tbaa !26
  %209 = lshr i128 %206, 8
  %210 = add nuw nsw i64 %205, 1
  %211 = icmp eq i64 %210, %203
  br i1 %211, label %440, label %204
212:                                              ; preds = %107
  %213 = sub i64 %2, %105
  %214 = icmp eq i64 %213, 3
  br i1 %214, label %215, label %294

215:                                              ; preds = %212
  %216 = getelementptr inbounds i8, ptr %1, i64 %105
  %217 = load i8, ptr %216, align 1, !tbaa !26
  switch i8 %217, label %294 [
    i8 105, label %218
    i8 110, label %250
  ]

218:                                              ; preds = %215
  %219 = getelementptr inbounds i8, ptr %216, i64 1
  %220 = load i8, ptr %219, align 1, !tbaa !26
  %221 = icmp eq i8 %220, 110
  br i1 %221, label %222, label %248

222:                                              ; preds = %218
  %223 = getelementptr inbounds i8, ptr %216, i64 2
  %224 = load i8, ptr %223, align 1, !tbaa !26
  %225 = icmp eq i8 %224, 102
  br i1 %225, label %226, label %248

226:                                              ; preds = %222
  %227 = zext i1 %101 to i128
  %228 = add nsw i32 %92, -1
  %229 = zext i32 %228 to i128
  %230 = shl nuw i128 %227, %229
  %231 = add nsw i32 %92, -6
  %232 = zext i32 %93 to i128
  %233 = select i1 %98, i32 %94, i32 %231
  %234 = select i1 %98, i128 %232, i128 30
  %235 = zext i32 %233 to i128
  %236 = shl i128 %234, %235
  %237 = or i128 %230, %236
  %238 = lshr exact i32 %92, 3
  %239 = zext i32 %238 to i64
  br label %240

240:                                              ; preds = %240, %226
  %241 = phi i64 [ 0, %226 ], [ %246, %240 ]
  %242 = phi i128 [ %237, %226 ], [ %245, %240 ]
  %243 = trunc i128 %242 to i8
  %244 = getelementptr inbounds i8, ptr %0, i64 %241
  store i8 %243, ptr %244, align 1, !tbaa !26
  %245 = lshr i128 %242, 8
  %246 = add nuw nsw i64 %241, 1
  %247 = icmp eq i64 %246, %239
  br i1 %247, label %440, label %240
248:                                              ; preds = %222, %218
  %249 = icmp eq i8 %217, 110
  br i1 %249, label %250, label %294

250:                                              ; preds = %215, %248
  %251 = getelementptr inbounds i8, ptr %216, i64 1
  %252 = load i8, ptr %251, align 1, !tbaa !26
  %253 = icmp eq i8 %252, 97
  br i1 %253, label %254, label %294

254:                                              ; preds = %250
  %255 = getelementptr inbounds i8, ptr %216, i64 2
  %256 = load i8, ptr %255, align 1, !tbaa !26
  %257 = icmp eq i8 %256, 110
  br i1 %257, label %258, label %294

258:                                              ; preds = %254
  br i1 %98, label %259, label %270

259:                                              ; preds = %258
  %260 = zext i32 %93 to i128
  %261 = zext i32 %94 to i128
  %262 = shl nuw i128 %260, %261
  %263 = add nsw i32 %94, -1
  %264 = zext i32 %263 to i128
  %265 = shl nuw i128 1, %264
  %266 = or i128 %265, %262
  %267 = trunc i128 %266 to i64
  %268 = lshr i128 %266, 64
  %269 = trunc i128 %268 to i64
  br label %277

270:                                              ; preds = %258
  %271 = add nsw i32 %92, -6
  %272 = zext i32 %271 to i128
  %273 = shl i128 31, %272
  %274 = trunc i128 %273 to i64
  %275 = lshr i128 %273, 64
  %276 = trunc i128 %275 to i64
  br label %277

277:                                              ; preds = %270, %259
  %278 = phi i64 [ %274, %270 ], [ %267, %259 ]
  %279 = phi i64 [ %276, %270 ], [ %269, %259 ]
  %280 = lshr exact i32 %92, 3
  %281 = zext i64 %279 to i128
  %282 = shl nuw i128 %281, 64
  %283 = zext i64 %278 to i128
  %284 = or i128 %282, %283
  %285 = zext i32 %280 to i64
  br label %286

286:                                              ; preds = %286, %277
  %287 = phi i64 [ 0, %277 ], [ %292, %286 ]
  %288 = phi i128 [ %284, %277 ], [ %291, %286 ]
  %289 = trunc i128 %288 to i8
  %290 = getelementptr inbounds i8, ptr %0, i64 %287
  store i8 %289, ptr %290, align 1, !tbaa !26
  %291 = lshr i128 %288, 8
  %292 = add nuw nsw i64 %287, 1
  %293 = icmp eq i64 %292, %285
  br i1 %293, label %440, label %286
294:                                              ; preds = %215, %254, %250, %248, %212
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %7) #10
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %7, i8 0, i64 5604, i1 false), !alias.scope !136
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %8) #10
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %8, i8 0, i64 5604, i1 false), !alias.scope !139
  %295 = getelementptr inbounds i8, ptr %8, i64 4
  store i32 1, ptr %8, align 4, !tbaa !34, !alias.scope !139
  store i32 1, ptr %295, align 4, !tbaa !24, !alias.scope !139
  %296 = call fastcc i32 @decimal_digits(ptr noundef nonnull %1, i64 noundef %2, ptr noundef %6, ptr noundef nonnull %7, ptr noundef null) #11
  %297 = icmp slt i32 %296, 0
  br i1 %297, label %437, label %298

298:                                              ; preds = %294
  %299 = load i64, ptr %6, align 8, !tbaa !133
  %300 = icmp ult i64 %299, %2
  br i1 %300, label %301, label %309

301:                                              ; preds = %298
  %302 = getelementptr inbounds i8, ptr %1, i64 %299
  %303 = load i8, ptr %302, align 1, !tbaa !26
  %304 = icmp eq i8 %303, 46
  br i1 %304, label %305, label %309

305:                                              ; preds = %301
  %306 = add nuw nsw i64 %299, 1
  store i64 %306, ptr %6, align 8, !tbaa !133
  %307 = call fastcc i32 @decimal_digits(ptr noundef nonnull %1, i64 noundef %2, ptr noundef %6, ptr noundef nonnull %7, ptr noundef null) #11
  %308 = icmp slt i32 %307, 0
  br i1 %308, label %437, label %309

309:                                              ; preds = %305, %301, %298
  %310 = phi i32 [ %307, %305 ], [ 0, %301 ], [ 0, %298 ]
  %311 = or i32 %310, %296
  %312 = icmp eq i32 %311, 0
  br i1 %312, label %437, label %313

313:                                              ; preds = %309
  call void @llvm.lifetime.start.p0(i64 4, ptr nonnull %9) #10
  store i32 0, ptr %9, align 4, !tbaa !24
  %314 = load i64, ptr %6, align 8, !tbaa !133
  %315 = icmp ult i64 %314, %2
  br i1 %315, label %316, label %345

316:                                              ; preds = %313
  %317 = getelementptr inbounds i8, ptr %1, i64 %314
  %318 = load i8, ptr %317, align 1, !tbaa !26
  switch i8 %318, label %345 [
    i8 101, label %319
    i8 69, label %319
  ]

319:                                              ; preds = %316, %316
  %320 = add i64 %314, 1
  store i64 %320, ptr %6, align 8, !tbaa !133
  %321 = icmp ult i64 %320, %2
  br i1 %321, label %322, label %326

322:                                              ; preds = %319
  %323 = getelementptr inbounds i8, ptr %1, i64 %320
  %324 = load i8, ptr %323, align 1, !tbaa !26
  %325 = icmp eq i8 %324, 45
  br label %326

326:                                              ; preds = %322, %319
  %327 = phi i1 [ false, %319 ], [ %325, %322 ]
  br i1 %321, label %328, label %335

328:                                              ; preds = %326
  br i1 %327, label %333, label %329

329:                                              ; preds = %328
  %330 = getelementptr inbounds i8, ptr %1, i64 %320
  %331 = load i8, ptr %330, align 1, !tbaa !26
  %332 = icmp eq i8 %331, 43
  br i1 %332, label %333, label %335

333:                                              ; preds = %329, %328
  %334 = add i64 %314, 2
  store i64 %334, ptr %6, align 8, !tbaa !133
  br label %335

335:                                              ; preds = %333, %329, %326
  %336 = call fastcc i32 @decimal_digits(ptr noundef nonnull %1, i64 noundef %2, ptr noundef %6, ptr noundef null, ptr noundef nonnull %9) #11
  %337 = icmp sgt i32 %336, 0
  %338 = select i1 %337, i1 %327, i1 false
  %339 = zext i1 %337 to i32
  br i1 %338, label %340, label %343

340:                                              ; preds = %335
  %341 = load i32, ptr %9, align 4, !tbaa !24
  %342 = sub nsw i32 0, %341
  store i32 %342, ptr %9, align 4, !tbaa !24
  br label %343

343:                                              ; preds = %335, %340
  %344 = phi i32 [ %339, %335 ], [ 1, %340 ]
  br i1 %337, label %345, label %435

345:                                              ; preds = %316, %343, %313
  %346 = load i64, ptr %6, align 8, !tbaa !133
  %347 = icmp eq i64 %346, %2
  br i1 %347, label %348, label %435

348:                                              ; preds = %345
  %349 = load i32, ptr %9, align 4, !tbaa !24
  %350 = sub nsw i32 %349, %310
  store i32 %350, ptr %9, align 4, !tbaa !24
  %351 = load i32, ptr %7, align 4, !tbaa !34
  %352 = icmp eq i32 %351, 0
  br i1 %352, label %353, label %371

353:                                              ; preds = %348
  %354 = call fastcc { i64, i64 } @pack(ptr noundef %7, ptr noundef %8, i32 noundef %350, i32 noundef %102, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %5) #11
  %355 = extractvalue { i64, i64 } %354, 1
  %356 = extractvalue { i64, i64 } %354, 0
  %357 = lshr exact i32 %92, 3
  %358 = zext i64 %355 to i128
  %359 = shl nuw i128 %358, 64
  %360 = zext i64 %356 to i128
  %361 = or i128 %359, %360
  %362 = zext i32 %357 to i64
  br label %363

363:                                              ; preds = %363, %353
  %364 = phi i64 [ 0, %353 ], [ %369, %363 ]
  %365 = phi i128 [ %361, %353 ], [ %368, %363 ]
  %366 = trunc i128 %365 to i8
  %367 = getelementptr inbounds i8, ptr %0, i64 %364
  store i8 %366, ptr %367, align 1, !tbaa !26
  %368 = lshr i128 %365, 8
  %369 = add nuw nsw i64 %364, 1
  %370 = icmp eq i64 %369, %362
  br i1 %370, label %435, label %363
371:                                              ; preds = %348
  %372 = call fastcc i32 @magnitude(ptr noundef %7, ptr noundef %8, i32 noundef 10) #11
  %373 = add nsw i32 %372, %350
  %374 = add nuw nsw i32 %97, %95
  br i1 %99, label %375, label %377

375:                                              ; preds = %371
  %376 = add nsw i32 %374, -1
  br label %381

377:                                              ; preds = %371
  %378 = mul nuw nsw i32 %374, 30103
  %379 = udiv i32 %378, 100000
  %380 = add nuw nsw i32 %379, 1
  br label %381

381:                                              ; preds = %377, %375
  %382 = phi i32 [ %376, %375 ], [ %380, %377 ]
  br i1 %99, label %383, label %385

383:                                              ; preds = %381
  %384 = add nsw i32 %96, -1
  br label %389

385:                                              ; preds = %381
  %386 = mul nsw i32 %96, 30103
  %387 = sdiv i32 %386, 100000
  %388 = add nsw i32 %387, -2
  br label %389

389:                                              ; preds = %385, %383
  %390 = phi i32 [ %384, %383 ], [ %388, %385 ]
  %391 = icmp sgt i32 %373, %382
  br i1 %391, label %435, label %392

392:                                              ; preds = %389
  %393 = icmp slt i32 %373, %390
  br i1 %393, label %394, label %395

394:                                              ; preds = %392
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %7, i8 0, i64 5604, i1 false)
  br label %401

395:                                              ; preds = %392
  br i1 %98, label %396, label %403

396:                                              ; preds = %395
  %397 = icmp sgt i32 %350, -1
  br i1 %397, label %398, label %399

398:                                              ; preds = %396
  call fastcc void @power(ptr noundef %7, i32 noundef 10, i32 noundef %350) #11
  br label %401

399:                                              ; preds = %396
  %400 = sub nsw i32 0, %350
  call fastcc void @power(ptr noundef %8, i32 noundef 10, i32 noundef %400) #11
  br label %401

401:                                              ; preds = %398, %399, %394
  %402 = phi i32 [ %96, %394 ], [ 0, %399 ], [ 0, %398 ]
  store i32 %402, ptr %9, align 4, !tbaa !24
  br label %403

403:                                              ; preds = %401, %395
  %404 = load i32, ptr %9, align 4, !tbaa !24
  %405 = call fastcc { i64, i64 } @pack(ptr noundef %7, ptr noundef %8, i32 noundef %404, i32 noundef %102, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %5) #11
  %406 = extractvalue { i64, i64 } %405, 0
  %407 = extractvalue { i64, i64 } %405, 1
  %408 = zext i64 %407 to i128
  %409 = shl nuw i128 %408, 64
  %410 = zext i64 %406 to i128
  %411 = or i128 %409, %410
  %412 = add nsw i32 %92, -1
  %413 = zext i32 %412 to i128
  %414 = shl nsw i128 -1, %413
  %415 = xor i128 %414, -1
  %416 = and i128 %411, %415
  %417 = zext i32 %93 to i128
  %418 = add nsw i32 %92, -6
  %419 = select i1 %98, i32 %94, i32 %418
  %420 = select i1 %98, i128 %417, i128 30
  %421 = zext i32 %419 to i128
  %422 = shl i128 %420, %421
  %423 = icmp eq i128 %416, %422
  br i1 %423, label %435, label %424

424:                                              ; preds = %403
  %425 = lshr exact i32 %92, 3
  %426 = zext i32 %425 to i64
  br label %427

427:                                              ; preds = %427, %424
  %428 = phi i64 [ 0, %424 ], [ %433, %427 ]
  %429 = phi i128 [ %411, %424 ], [ %432, %427 ]
  %430 = trunc i128 %429 to i8
  %431 = getelementptr inbounds i8, ptr %0, i64 %428
  store i8 %430, ptr %431, align 1, !tbaa !26
  %432 = lshr i128 %429, 8
  %433 = add nuw nsw i64 %428, 1
  %434 = icmp eq i64 %433, %426
  br i1 %434, label %435, label %427
435:                                              ; preds = %427, %363, %389, %403, %345, %343
  %436 = phi i32 [ %344, %343 ], [ 0, %345 ], [ 0, %389 ], [ 0, %403 ], [ 1, %363 ], [ 1, %427 ]
  call void @llvm.lifetime.end.p0(i64 4, ptr nonnull %9) #10
  br label %437

437:                                              ; preds = %435, %305, %309, %294
  %438 = phi i32 [ 0, %294 ], [ %436, %435 ], [ 0, %305 ], [ 0, %309 ]
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %8) #10
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %7) #10
  br label %440

439:                                              ; preds = %188, %154
  store i64 %150, ptr %6, align 8
  br label %440

440:                                              ; preds = %204, %286, %240, %439, %125, %108, %104, %437
  %441 = phi i32 [ %438, %437 ], [ 0, %104 ], [ 0, %108 ], [ 0, %125 ], [ 0, %439 ], [ 1, %240 ], [ 1, %286 ], [ 1, %204 ]
  call void @llvm.lifetime.end.p0(i64 8, ptr nonnull %6) #10
  call void @llvm.lifetime.end.p0(i64 36, ptr nonnull %5) #10
  br label %442

442:                                              ; preds = %4, %440
  %443 = phi i32 [ %441, %440 ], [ 0, %4 ]
  ret i32 %443
}

; Function Attrs: nounwind memory(argmem: readwrite, inaccessiblemem: write)
define internal fastcc i32 @decimal_digits(ptr noundef readonly %0, i64 noundef %1, ptr noundef nonnull %2, ptr noundef %3, ptr noundef %4) unnamed_addr #2 {
  %6 = load i64, ptr %2, align 8, !tbaa !133
  %7 = icmp ult i64 %6, %1
  br i1 %7, label %8, label %105

8:                                                ; preds = %5
  %9 = icmp eq ptr %3, null
  %10 = getelementptr inbounds i8, ptr %3, i64 4
  br label %11

11:                                               ; preds = %8, %101
  %12 = phi i32 [ 0, %8 ], [ %99, %101 ]
  %13 = phi i64 [ %6, %8 ], [ %98, %101 ]
  %14 = getelementptr inbounds i8, ptr %0, i64 %13
  %15 = load i8, ptr %14, align 1, !tbaa !26
  %16 = icmp eq i8 %15, 95
  br i1 %16, label %17, label %27

17:                                               ; preds = %11
  %18 = icmp eq i32 %12, 0
  br i1 %18, label %97, label %19

19:                                               ; preds = %17
  %20 = add nuw nsw i64 %13, 1
  %21 = icmp eq i64 %20, %1
  br i1 %21, label %97, label %22

22:                                               ; preds = %19
  %23 = getelementptr inbounds i8, ptr %0, i64 %20
  %24 = load i8, ptr %23, align 1, !tbaa !26
  %25 = add i8 %24, -58
  %26 = icmp ult i8 %25, -10
  br i1 %26, label %97, label %93
27:                                               ; preds = %11
  %28 = add i8 %15, -58
  %29 = icmp ult i8 %28, -10
  br i1 %29, label %97, label %30

30:                                               ; preds = %27
  %31 = add nsw i8 %15, -48
  %32 = zext i8 %31 to i32
  br i1 %9, label %84, label %33

33:                                               ; preds = %30
  %34 = load i32, ptr %3, align 4, !tbaa !34
  %35 = icmp sgt i32 %34, 0
  br i1 %35, label %36, label %38

36:                                               ; preds = %33
  %37 = zext i32 %34 to i64
  br label %41

38:                                               ; preds = %41, %33
  %39 = phi i64 [ 0, %33 ], [ %50, %41 ]
  %40 = icmp eq i64 %39, 0
  br i1 %40, label %61, label %53

41:                                               ; preds = %41, %36
  %42 = phi i64 [ 0, %36 ], [ %51, %41 ]
  %43 = phi i64 [ 0, %36 ], [ %50, %41 ]
  %44 = getelementptr inbounds [1400 x i32], ptr %10, i64 0, i64 %42
  %45 = load i32, ptr %44, align 4, !tbaa !24
  %46 = zext i32 %45 to i64
  %47 = mul nuw nsw i64 %46, 10
  %48 = add nuw nsw i64 %47, %43
  %49 = trunc i64 %48 to i32
  store i32 %49, ptr %44, align 4, !tbaa !24
  %50 = lshr i64 %48, 32
  %51 = add nuw nsw i64 %42, 1
  %52 = icmp eq i64 %51, %37
  br i1 %52, label %38, label %41
53:                                               ; preds = %38
  %54 = icmp eq i32 %34, 1400
  br i1 %54, label %55, label %56

55:                                               ; preds = %53
  tail call void @llvm.trap()
  unreachable

56:                                               ; preds = %53
  %57 = trunc i64 %39 to i32
  %58 = add nsw i32 %34, 1
  store i32 %58, ptr %3, align 4, !tbaa !34
  %59 = sext i32 %34 to i64
  %60 = getelementptr inbounds [1400 x i32], ptr %10, i64 0, i64 %59
  store i32 %57, ptr %60, align 4, !tbaa !24
  br label %61

61:                                               ; preds = %38, %56
  %62 = icmp eq i8 %31, 0
  br i1 %62, label %90, label %63

63:                                               ; preds = %61
  %64 = zext i8 %31 to i64
  br label %65

65:                                               ; preds = %63, %75
  %66 = phi i64 [ 0, %63 ], [ %82, %75 ]
  %67 = phi i64 [ %64, %63 ], [ %81, %75 ]
  %68 = load i32, ptr %3, align 4, !tbaa !34
  %69 = zext i32 %68 to i64
  %70 = icmp eq i64 %66, %69
  br i1 %70, label %71, label %75

71:                                               ; preds = %65
  %72 = add nsw i32 %68, 1
  store i32 %72, ptr %3, align 4, !tbaa !34
  %73 = sext i32 %68 to i64
  %74 = getelementptr inbounds [1400 x i32], ptr %10, i64 0, i64 %73
  store i32 0, ptr %74, align 4, !tbaa !24
  br label %75

75:                                               ; preds = %71, %65
  %76 = getelementptr inbounds [1400 x i32], ptr %10, i64 0, i64 %66
  %77 = load i32, ptr %76, align 4, !tbaa !24
  %78 = zext i32 %77 to i64
  %79 = add nuw nsw i64 %67, %78
  %80 = trunc i64 %79 to i32
  store i32 %80, ptr %76, align 4, !tbaa !24
  %81 = lshr i64 %79, 32
  %82 = add nuw nsw i64 %66, 1
  %83 = icmp samesign ult i64 %79, 4294967296
  br i1 %83, label %90, label %65
84:                                               ; preds = %30
  %85 = load i32, ptr %4, align 4, !tbaa !24
  %86 = icmp slt i32 %85, 100000
  br i1 %86, label %87, label %90

87:                                               ; preds = %84
  %88 = mul nsw i32 %85, 10
  %89 = add nsw i32 %88, %32
  store i32 %89, ptr %4, align 4, !tbaa !24
  br label %90

90:                                               ; preds = %75, %61, %84, %87
  %91 = add nsw i32 %12, 1
  %92 = add i64 %13, 1
  br label %93

93:                                               ; preds = %22, %90
  %94 = phi i64 [ %92, %90 ], [ %20, %22 ]
  %95 = phi i32 [ %91, %90 ], [ %12, %22 ]
  %96 = phi i32 [ 0, %90 ], [ 2, %22 ]
  store i64 %94, ptr %2, align 8, !tbaa !133
  br label %97

97:                                               ; preds = %93, %27, %17, %19, %22
  %98 = phi i64 [ %13, %22 ], [ %13, %19 ], [ %13, %17 ], [ %13, %27 ], [ %94, %93 ]
  %99 = phi i32 [ %12, %22 ], [ %12, %19 ], [ %12, %17 ], [ %12, %27 ], [ %95, %93 ]
  %100 = phi i32 [ 1, %22 ], [ 1, %19 ], [ 1, %17 ], [ 3, %27 ], [ %96, %93 ]
  switch i32 %100, label %103 [
    i32 0, label %101
    i32 2, label %101
    i32 3, label %104
    i32 1, label %105
  ]

101:                                              ; preds = %97, %97
  %102 = icmp ult i64 %98, %1
  br i1 %102, label %11, label %104
103:                                              ; preds = %97
  unreachable

104:                                              ; preds = %97, %101
  br label %105

105:                                              ; preds = %97, %104, %5
  %106 = phi i32 [ 0, %5 ], [ %99, %104 ], [ -1, %97 ]
  ret i32 %106
}

; Function Attrs: nounwind memory(argmem: readwrite, inaccessiblemem: write)
define internal fastcc i32 @magnitude(ptr noundef nonnull readonly %0, ptr noundef nonnull readonly %1, i32 noundef %2) unnamed_addr #2 {
  %4 = alloca %struct.tzrt_big, align 4
  %5 = alloca %struct.tzrt_big, align 4
  %6 = load i32, ptr %0, align 4, !tbaa !34
  %7 = icmp eq i32 %6, 0
  br i1 %7, label %18, label %8

8:                                                ; preds = %3
  %9 = add nsw i32 %6, -1
  %10 = shl nsw i32 %9, 5
  %11 = add nsw i32 %10, 32
  %12 = getelementptr inbounds i8, ptr %0, i64 4
  %13 = sext i32 %9 to i64
  %14 = getelementptr inbounds [1400 x i32], ptr %12, i64 0, i64 %13
  %15 = load i32, ptr %14, align 4, !tbaa !24
  %16 = tail call i32 @llvm.ctlz.i32(i32 %15, i1 true)
  %17 = sub nsw i32 %11, %16
  br label %18

18:                                               ; preds = %3, %8
  %19 = phi i32 [ %17, %8 ], [ 0, %3 ]
  %20 = load i32, ptr %1, align 4, !tbaa !34
  %21 = icmp eq i32 %20, 0
  br i1 %21, label %32, label %22

22:                                               ; preds = %18
  %23 = add nsw i32 %20, -1
  %24 = getelementptr inbounds i8, ptr %1, i64 4
  %25 = sext i32 %23 to i64
  %26 = getelementptr inbounds [1400 x i32], ptr %24, i64 0, i64 %25
  %27 = load i32, ptr %26, align 4, !tbaa !24
  %28 = tail call i32 @llvm.ctlz.i32(i32 %27, i1 true)
  %29 = shl i32 %23, 5
  %30 = sub i32 %28, %29
  %31 = add i32 %30, -32
  br label %32

32:                                               ; preds = %18, %22
  %33 = phi i32 [ %31, %22 ], [ 0, %18 ]
  %34 = add i32 %33, %19
  %35 = icmp eq i32 %2, 10
  br i1 %35, label %36, label %39

36:                                               ; preds = %32
  %37 = mul nsw i32 %34, 30103
  %38 = sdiv i32 %37, 100000
  br label %39

39:                                               ; preds = %36, %32
  %40 = phi i32 [ %38, %36 ], [ %34, %32 ]
  %41 = getelementptr inbounds i8, ptr %5, i64 4
  %42 = getelementptr inbounds i8, ptr %1, i64 4
  %43 = getelementptr inbounds i8, ptr %0, i64 4
  %44 = sext i32 %6 to i64
  %45 = icmp eq i32 %6, 0
  br label %46

46:                                               ; preds = %91, %39
  %47 = phi i32 [ %40, %39 ], [ %92, %91 ]
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %5) #10
  %48 = icmp sgt i32 %47, -1
  %49 = select i1 %48, ptr %1, ptr %0
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %5, ptr noundef nonnull readonly align 4 dereferenceable(5604) %49, i64 5604, i1 false)
  %50 = tail call i32 @llvm.abs.i32(i32 %47, i1 true)
  call fastcc void @power(ptr noundef %5, i32 noundef %2, i32 noundef %50) #11
  %51 = load i32, ptr %5, align 4, !tbaa !34
  br i1 %48, label %52, label %69

52:                                               ; preds = %46
  %53 = icmp eq i32 %6, %51
  br i1 %53, label %54, label %55

54:                                               ; preds = %52
  br i1 %45, label %93, label %59

55:                                               ; preds = %52
  %56 = icmp slt i32 %6, %51
  br label %89

57:                                               ; preds = %59
  %58 = icmp eq i64 %61, 0
  br i1 %58, label %93, label %59
59:                                               ; preds = %54, %57
  %60 = phi i64 [ %61, %57 ], [ %44, %54 ]
  %61 = add nsw i64 %60, -1
  %62 = getelementptr inbounds [1400 x i32], ptr %43, i64 0, i64 %61
  %63 = load i32, ptr %62, align 4, !tbaa !24
  %64 = getelementptr inbounds [1400 x i32], ptr %41, i64 0, i64 %61
  %65 = load i32, ptr %64, align 4, !tbaa !24
  %66 = icmp eq i32 %63, %65
  br i1 %66, label %57, label %67
67:                                               ; preds = %59
  %68 = icmp ult i32 %63, %65
  br label %89

69:                                               ; preds = %46
  %70 = icmp eq i32 %51, %20
  br i1 %70, label %71, label %75

71:                                               ; preds = %69
  %72 = icmp eq i32 %51, 0
  br i1 %72, label %93, label %73

73:                                               ; preds = %71
  %74 = sext i32 %51 to i64
  br label %79

75:                                               ; preds = %69
  %76 = icmp slt i32 %51, %20
  br label %89

77:                                               ; preds = %79
  %78 = icmp eq i64 %81, 0
  br i1 %78, label %93, label %79
79:                                               ; preds = %73, %77
  %80 = phi i64 [ %74, %73 ], [ %81, %77 ]
  %81 = add nsw i64 %80, -1
  %82 = getelementptr inbounds [1400 x i32], ptr %41, i64 0, i64 %81
  %83 = load i32, ptr %82, align 4, !tbaa !24
  %84 = getelementptr inbounds [1400 x i32], ptr %42, i64 0, i64 %81
  %85 = load i32, ptr %84, align 4, !tbaa !24
  %86 = icmp eq i32 %83, %85
  br i1 %86, label %77, label %87
87:                                               ; preds = %79
  %88 = icmp ult i32 %83, %85
  br label %89

89:                                               ; preds = %55, %67, %75, %87
  %90 = phi i1 [ %56, %55 ], [ %68, %67 ], [ %76, %75 ], [ %88, %87 ]
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %5) #10
  br i1 %90, label %91, label %94

91:                                               ; preds = %89
  %92 = add nsw i32 %47, -1
  br label %46
93:                                               ; preds = %71, %54, %77, %57
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %5) #10
  br label %94

94:                                               ; preds = %89, %93
  %95 = getelementptr inbounds i8, ptr %4, i64 4
  %96 = icmp eq i32 %6, 0
  br label %97

97:                                               ; preds = %94, %145
  %98 = phi i32 [ %99, %145 ], [ %47, %94 ]
  %99 = add nsw i32 %98, 1
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %4) #10
  %100 = icmp sgt i32 %98, -2
  %101 = select i1 %100, ptr %1, ptr %0
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %4, ptr noundef nonnull readonly align 4 dereferenceable(5604) %101, i64 5604, i1 false)
  %102 = tail call i32 @llvm.abs.i32(i32 %99, i1 true)
  call fastcc void @power(ptr noundef %4, i32 noundef %2, i32 noundef %102) #11
  %103 = load i32, ptr %4, align 4, !tbaa !34
  br i1 %100, label %104, label %123

104:                                              ; preds = %97
  %105 = icmp eq i32 %6, %103
  br i1 %105, label %106, label %107

106:                                              ; preds = %104
  br i1 %96, label %145, label %112

107:                                              ; preds = %104
  %108 = icmp slt i32 %6, %103
  %109 = select i1 %108, i32 -1, i32 1
  br label %145

110:                                              ; preds = %112
  %111 = icmp eq i64 %114, 0
  br i1 %111, label %145, label %112
112:                                              ; preds = %106, %110
  %113 = phi i64 [ %114, %110 ], [ %44, %106 ]
  %114 = add nsw i64 %113, -1
  %115 = getelementptr inbounds [1400 x i32], ptr %43, i64 0, i64 %114
  %116 = load i32, ptr %115, align 4, !tbaa !24
  %117 = getelementptr inbounds [1400 x i32], ptr %95, i64 0, i64 %114
  %118 = load i32, ptr %117, align 4, !tbaa !24
  %119 = icmp eq i32 %116, %118
  br i1 %119, label %110, label %120
120:                                              ; preds = %112
  %121 = icmp ult i32 %116, %118
  %122 = select i1 %121, i32 -1, i32 1
  br label %145

123:                                              ; preds = %97
  %124 = icmp eq i32 %103, %20
  br i1 %124, label %125, label %129

125:                                              ; preds = %123
  %126 = icmp eq i32 %103, 0
  br i1 %126, label %145, label %127

127:                                              ; preds = %125
  %128 = sext i32 %103 to i64
  br label %134

129:                                              ; preds = %123
  %130 = icmp slt i32 %103, %20
  %131 = select i1 %130, i32 -1, i32 1
  br label %145

132:                                              ; preds = %134
  %133 = icmp eq i64 %136, 0
  br i1 %133, label %145, label %134
134:                                              ; preds = %127, %132
  %135 = phi i64 [ %128, %127 ], [ %136, %132 ]
  %136 = add nsw i64 %135, -1
  %137 = getelementptr inbounds [1400 x i32], ptr %95, i64 0, i64 %136
  %138 = load i32, ptr %137, align 4, !tbaa !24
  %139 = getelementptr inbounds [1400 x i32], ptr %42, i64 0, i64 %136
  %140 = load i32, ptr %139, align 4, !tbaa !24
  %141 = icmp eq i32 %138, %140
  br i1 %141, label %132, label %142
142:                                              ; preds = %134
  %143 = icmp ult i32 %138, %140
  %144 = select i1 %143, i32 -1, i32 1
  br label %145

145:                                              ; preds = %132, %110, %125, %106, %107, %120, %129, %142
  %146 = phi i32 [ %109, %107 ], [ %122, %120 ], [ %131, %129 ], [ %144, %142 ], [ 0, %106 ], [ 0, %125 ], [ 0, %110 ], [ 0, %132 ]
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %4) #10
  %147 = icmp sgt i32 %146, -1
  br i1 %147, label %97, label %148
148:                                              ; preds = %145
  ret i32 %98
}

; Function Attrs: mustprogress nocallback nofree nounwind willreturn memory(argmem: write)
declare void @llvm.memset.p0.i64(ptr writeonly, i8, i64, i1 immarg) #5

; Function Attrs: cold noreturn nounwind memory(inaccessiblemem: write)
declare void @llvm.trap() #6

; Function Attrs: mustprogress nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i32 @llvm.ctlz.i32(i32, i1 immarg) #7

; Function Attrs: nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i32 @llvm.smin.i32(i32, i32) #8

; Function Attrs: nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i32 @llvm.scmp.i32.i32(i32, i32) #8

; Function Attrs: nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i32 @llvm.abs.i32(i32, i1 immarg) #8

; Function Attrs: nocallback nofree nosync nounwind willreturn memory(inaccessiblemem: readwrite)
declare void @llvm.experimental.noalias.scope.decl(metadata) #9

; Function Attrs: nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i32 @llvm.smax.i32(i32, i32) #8

attributes #0 = { nounwind memory(argmem: readwrite, inaccessiblemem: readwrite) "min-legal-vector-width"="0" "no-builtins" "no-trapping-math"="true" "stack-protector-buffer-size"="8" "tune-cpu"="generic" }
attributes #1 = { mustprogress nocallback nofree nosync nounwind willreturn memory(argmem: readwrite) }
attributes #2 = { nounwind memory(argmem: readwrite, inaccessiblemem: write) "min-legal-vector-width"="0" "no-builtins" "no-trapping-math"="true" "stack-protector-buffer-size"="8" "tune-cpu"="generic" }
attributes #3 = { mustprogress nocallback nofree nounwind willreturn memory(argmem: readwrite) }
attributes #4 = { nounwind "min-legal-vector-width"="0" "no-builtins" "no-trapping-math"="true" "stack-protector-buffer-size"="8" "tune-cpu"="generic" }
attributes #5 = { mustprogress nocallback nofree nounwind willreturn memory(argmem: write) }
attributes #6 = { cold noreturn nounwind memory(inaccessiblemem: write) }
attributes #7 = { mustprogress nocallback nofree nosync nounwind speculatable willreturn memory(none) }
attributes #8 = { nocallback nofree nosync nounwind speculatable willreturn memory(none) }
attributes #9 = { nocallback nofree nosync nounwind willreturn memory(inaccessiblemem: readwrite) }
attributes #10 = { nounwind }
attributes #11 = { nobuiltin "no-builtins" }


!0 = !{i32 1, !"wchar_size", i32 4}
!1 = !{i32 8, !"PIC Level", i32 2}
!2 = !{i32 7, !"PIE Level", i32 2}
!3 = !{!"Apple clang version 21.0.0 (clang-2100.3.34.2)"}
!4 = !{!5, !6, i64 0}
!5 = !{!"", !6, i64 0, !6, i64 4, !6, i64 8, !6, i64 12, !6, i64 16, !6, i64 20, !6, i64 24, !6, i64 28, !6, i64 32}
!6 = !{!"int", !7, i64 0}
!7 = !{!"omnipotent char", !8, i64 0}
!8 = !{!"Simple C/C++ TBAA"}
!9 = !{!10}
!10 = distinct !{!10, !11, !"format: argument 0"}
!11 = distinct !{!11, !"format"}
!12 = !{!5, !6, i64 4}
!13 = !{!5, !6, i64 8}
!14 = !{!5, !6, i64 12}
!15 = !{!5, !6, i64 16}
!16 = !{!5, !6, i64 20}
!17 = !{!5, !6, i64 24}
!18 = !{!5, !6, i64 28}
!19 = !{!5, !6, i64 32}
!20 = !{!21, !6, i64 5608}
!21 = !{!"", !22, i64 0, !6, i64 5604, !6, i64 5608, !6, i64 5612}
!22 = !{!"", !6, i64 0, !7, i64 4}
!23 = !{!21, !6, i64 5612}
!24 = !{!6, !6, i64 0}
!25 = !{!21, !6, i64 0}
!26 = !{!7, !7, i64 0}
!27 = distinct !{!27, !28, !29}
!28 = !{!"llvm.loop.mustprogress"}
!29 = !{!"llvm.loop.unroll.disable"}
!30 = !{i64 0, i64 4, !24, i64 4, i64 5600, !26}
!31 = !{!32}
!32 = distinct !{!32, !33, !"small: argument 0"}
!33 = distinct !{!33, !"small"}
!34 = !{!22, !6, i64 0}
!35 = !{!21, !6, i64 5604}
!36 = distinct !{!36, !28, !29}
!37 = distinct !{!37, !28, !29}
!38 = distinct !{!38, !28, !29}
!39 = distinct !{!39, !28, !29}
!40 = !{!41}
!41 = distinct !{!41, !42, !"multiply: argument 0"}
!42 = distinct !{!42, !"multiply"}
!43 = distinct !{!43, !28, !29}
!44 = distinct !{!44, !28, !29}
!45 = distinct !{!45, !28, !29}
!46 = !{!47}
!47 = distinct !{!47, !48, !"small: argument 0"}
!48 = distinct !{!48, !"small"}
!49 = distinct !{!49, !28, !29}
!50 = !{!51}
!51 = distinct !{!51, !52, !"small: argument 0"}
!52 = distinct !{!52, !"small"}
!53 = !{!54}
!54 = distinct !{!54, !55, !"small: argument 0"}
!55 = distinct !{!55, !"small"}
!56 = !{!57}
!57 = distinct !{!57, !58, !"small: argument 0"}
!58 = distinct !{!58, !"small"}
!59 = distinct !{!59, !28, !29}
!60 = distinct !{!60, !28, !29}
!61 = distinct !{!61, !28, !29}
!62 = distinct !{!62, !28, !29}
!63 = distinct !{!63, !28, !29}
!64 = distinct !{!64, !28, !29}
!65 = !{!66}
!66 = distinct !{!66, !67, !"rounded: argument 0"}
!67 = distinct !{!67, !"rounded"}
!68 = !{!69}
!69 = distinct !{!69, !70, !"small: argument 0"}
!70 = distinct !{!70, !"small"}
!71 = !{!72}
!72 = distinct !{!72, !73, !"small: argument 0"}
!73 = distinct !{!73, !"small"}
!74 = distinct !{!74, !28, !29}
!75 = distinct !{!75, !28, !29}
!76 = !{!77}
!77 = distinct !{!77, !78, !"format: argument 0"}
!78 = distinct !{!78, !"format"}
!79 = !{!80}
!80 = distinct !{!80, !81, !"format: argument 0"}
!81 = distinct !{!81, !"format"}
!82 = !{!83}
!83 = distinct !{!83, !84, !"format: argument 0"}
!84 = distinct !{!84, !"format"}
!85 = !{!86}
!86 = distinct !{!86, !87, !"small: argument 0"}
!87 = distinct !{!87, !"small"}
!88 = !{!89}
!89 = distinct !{!89, !90, !"small: argument 0"}
!90 = distinct !{!90, !"small"}
!91 = !{!92}
!92 = distinct !{!92, !93, !"multiply: argument 0"}
!93 = distinct !{!93, !"multiply"}
!94 = !{!95}
!95 = distinct !{!95, !96, !"small: argument 0"}
!96 = distinct !{!96, !"small"}
!97 = distinct !{!97, !28, !29}
!98 = distinct !{!98, !28, !29}
!99 = !{!100}
!100 = distinct !{!100, !101, !"format: argument 0"}
!101 = distinct !{!101, !"format"}
!102 = distinct !{!102, !28, !29}
!103 = distinct !{!103, !28, !29}
!104 = distinct !{!104, !28, !29}
!105 = !{!106}
!106 = distinct !{!106, !107, !"small: argument 0"}
!107 = distinct !{!107, !"small"}
!108 = !{!109}
!109 = distinct !{!109, !110, !"small: argument 0"}
!110 = distinct !{!110, !"small"}
!111 = !{!112}
!112 = distinct !{!112, !113, !"small: argument 0"}
!113 = distinct !{!113, !"small"}
!114 = !{!115}
!115 = distinct !{!115, !116, !"small: argument 0"}
!116 = distinct !{!116, !"small"}
!117 = distinct !{!117, !28, !29}
!118 = !{!119}
!119 = distinct !{!119, !120, !"small: argument 0"}
!120 = distinct !{!120, !"small"}
!121 = distinct !{!121, !29}
!122 = distinct !{!122, !28, !29}
!123 = distinct !{!123, !28, !29}
!124 = distinct !{!124, !28, !29}
!125 = distinct !{!125, !28, !29}
!126 = distinct !{!126, !28, !29}
!127 = distinct !{!127, !28, !29}
!128 = distinct !{!128, !28, !29}
!129 = distinct !{!129, !28, !29}
!130 = !{!131}
!131 = distinct !{!131, !132, !"format: argument 0"}
!132 = distinct !{!132, !"format"}
!133 = !{!134, !134, i64 0}
!134 = !{!"long long", !7, i64 0}
!135 = distinct !{!135, !28, !29}
!136 = !{!137}
!137 = distinct !{!137, !138, !"small: argument 0"}
!138 = distinct !{!138, !"small"}
!139 = !{!140}
!140 = distinct !{!140, !141, !"small: argument 0"}
!141 = distinct !{!141, !"small"}
!142 = distinct !{!142, !28, !29}
!143 = distinct !{!143, !28, !29}
!144 = distinct !{!144, !28, !29}
!145 = distinct !{!145, !28, !29}
