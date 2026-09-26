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
  br i1 %93, label %179, label %98

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
  %127 = icmp ult i128 %126, 18446744073709551616
  %128 = trunc i128 %126 to i64
  br i1 %127, label %144, label %129

129:                                              ; preds = %125, %129
  %130 = phi i64 [ %138, %129 ], [ 0, %125 ]
  %131 = phi i128 [ %133, %129 ], [ %126, %125 ]
  %132 = freeze i128 %131
  %133 = udiv i128 %132, 10
  %134 = mul i128 %133, 10
  %135 = sub i128 %132, %134
  %136 = trunc i128 %135 to i8
  %137 = or i8 %136, 48
  %138 = add nuw nsw i64 %130, 1
  %139 = getelementptr inbounds [40 x i8], ptr %12, i64 0, i64 %130
  store i8 %137, ptr %139, align 1, !tbaa !26
  %140 = icmp ult i128 %131, 184467440737095516160
  br i1 %140, label %141, label %129
141:                                              ; preds = %129
  %142 = trunc i128 %133 to i64
  %143 = trunc i64 %138 to i32
  br label %144

144:                                              ; preds = %141, %125
  %145 = phi i32 [ 0, %125 ], [ %143, %141 ]
  %146 = phi i64 [ %128, %125 ], [ %142, %141 ]
  %147 = zext i32 %145 to i64
  br label %148

148:                                              ; preds = %148, %144
  %149 = phi i32 [ %152, %148 ], [ %145, %144 ]
  %150 = phi i64 [ %156, %148 ], [ %147, %144 ]
  %151 = phi i64 [ %158, %148 ], [ %146, %144 ]
  %152 = add i32 %149, 1
  %153 = urem i64 %151, 10
  %154 = trunc i64 %153 to i8
  %155 = or i8 %154, 48
  %156 = add nuw nsw i64 %150, 1
  %157 = getelementptr inbounds [40 x i8], ptr %12, i64 0, i64 %150
  store i8 %155, ptr %157, align 1, !tbaa !26
  %158 = udiv i64 %151, 10
  %159 = icmp ult i64 %151, 10
  br i1 %159, label %160, label %148
160:                                              ; preds = %148
  br i1 %116, label %161, label %162

161:                                              ; preds = %160
  store i8 45, ptr %0, align 1, !tbaa !26
  br label %162

162:                                              ; preds = %161, %160
  %163 = phi i32 [ 1, %161 ], [ 0, %160 ]
  %164 = sext i32 %152 to i64
  %165 = zext i32 %163 to i64
  %166 = add i32 %163, %152
  %167 = zext i32 %166 to i64
  br label %168

168:                                              ; preds = %162, %168
  %169 = phi i64 [ %165, %162 ], [ %174, %168 ]
  %170 = phi i64 [ %164, %162 ], [ %171, %168 ]
  %171 = add nsw i64 %170, -1
  %172 = getelementptr inbounds [40 x i8], ptr %12, i64 0, i64 %171
  %173 = load i8, ptr %172, align 1, !tbaa !26
  %174 = add nuw nsw i64 %169, 1
  %175 = getelementptr inbounds i8, ptr %0, i64 %169
  store i8 %173, ptr %175, align 1, !tbaa !26
  %176 = icmp eq i64 %174, %167
  br i1 %176, label %177, label %168
177:                                              ; preds = %168
  %178 = trunc i64 %174 to i32
  call void @llvm.lifetime.end.p0(i64 40, ptr nonnull %12) #10
  br label %1281

179:                                              ; preds = %91
  call void @llvm.lifetime.start.p0(i64 5616, ptr nonnull %13) #10
  call fastcc void @decode(ptr sret(%struct.tzrt_number) align 4 %13, ptr noundef %1, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %11) #11
  %180 = getelementptr inbounds i8, ptr %13, i64 5608
  %181 = load i32, ptr %180, align 4, !tbaa !20
  %182 = icmp ne i32 %181, 0
  %183 = getelementptr inbounds i8, ptr %13, i64 5612
  %184 = load i32, ptr %183, align 4
  %185 = icmp ne i32 %184, 2
  %186 = select i1 %182, i1 %185, i1 false
  br i1 %186, label %187, label %188

187:                                              ; preds = %179
  store i8 45, ptr %0, align 1, !tbaa !26
  br label %188

188:                                              ; preds = %187, %179
  %189 = phi i32 [ 1, %187 ], [ 0, %179 ]
  %190 = icmp eq i32 %184, 0
  br i1 %190, label %204, label %191

191:                                              ; preds = %188
  %192 = icmp eq i32 %184, 2
  %193 = select i1 %192, ptr @.str, ptr @.str.1
  %194 = zext i32 %189 to i64
  br label %195

195:                                              ; preds = %191, %195
  %196 = phi i64 [ %194, %191 ], [ %200, %195 ]
  %197 = phi i64 [ 0, %191 ], [ %202, %195 ]
  %198 = getelementptr inbounds i8, ptr %193, i64 %197
  %199 = load i8, ptr %198, align 1, !tbaa !26
  %200 = add nuw nsw i64 %196, 1
  %201 = getelementptr inbounds i8, ptr %0, i64 %196
  store i8 %199, ptr %201, align 1, !tbaa !26
  %202 = add nuw nsw i64 %197, 1
  %203 = icmp eq i64 %202, 3
  br i1 %203, label %1277, label %195
204:                                              ; preds = %188
  %205 = load i32, ptr %13, align 4, !tbaa !25
  %206 = icmp eq i32 %205, 0
  br i1 %206, label %207, label %211

207:                                              ; preds = %204
  %208 = add nuw nsw i32 %189, 1
  %209 = zext i32 %189 to i64
  %210 = getelementptr inbounds i8, ptr %0, i64 %209
  store i8 48, ptr %210, align 1, !tbaa !26
  br label %1279

211:                                              ; preds = %204
  br i1 %95, label %212, label %1065

212:                                              ; preds = %211
  %213 = icmp sgt i32 %205, 4
  br i1 %213, label %217, label %214

214:                                              ; preds = %212
  %215 = getelementptr inbounds i8, ptr %13, i64 4
  %216 = sext i32 %205 to i64
  br label %223

217:                                              ; preds = %212
  tail call void @llvm.trap()
  unreachable

218:                                              ; preds = %223
  %219 = and i128 %230, 1
  %220 = icmp eq i128 %219, 0
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %5) #10
  %221 = shl i128 %231, 2
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %5, i8 0, i64 5604, i1 false), !alias.scope !106
  %222 = icmp eq i128 %221, 0
  br i1 %222, label %244, label %233

223:                                              ; preds = %223, %214
  %224 = phi i64 [ %216, %214 ], [ %226, %223 ]
  %225 = phi i128 [ 0, %214 ], [ %231, %223 ]
  %226 = add nsw i64 %224, -1
  %227 = shl i128 %225, 32
  %228 = getelementptr inbounds [1400 x i32], ptr %215, i64 0, i64 %226
  %229 = load i32, ptr %228, align 4, !tbaa !24
  %230 = zext i32 %229 to i128
  %231 = or i128 %227, %230
  %232 = icmp eq i64 %226, 0
  br i1 %232, label %218, label %223
233:                                              ; preds = %218
  %234 = getelementptr inbounds i8, ptr %5, i64 4
  br label %235

235:                                              ; preds = %235, %233
  %236 = phi i128 [ %221, %233 ], [ %242, %235 ]
  %237 = trunc i128 %236 to i32
  %238 = load i32, ptr %5, align 4, !tbaa !34, !alias.scope !106
  %239 = add nsw i32 %238, 1
  store i32 %239, ptr %5, align 4, !tbaa !34, !alias.scope !106
  %240 = sext i32 %238 to i64
  %241 = getelementptr inbounds [1400 x i32], ptr %234, i64 0, i64 %240
  store i32 %237, ptr %241, align 4, !tbaa !24, !alias.scope !106
  %242 = lshr i128 %236, 32
  %243 = icmp ult i128 %236, 4294967296
  br i1 %243, label %244, label %235
244:                                              ; preds = %235, %218
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %6) #10
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %6, i8 0, i64 5604, i1 false), !alias.scope !109
  %245 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 1, ptr %6, align 4, !tbaa !34, !alias.scope !109
  store i32 4, ptr %245, align 4, !tbaa !24, !alias.scope !109
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %7) #10
  %246 = zext i32 %96 to i128
  %247 = shl nuw nsw i128 1, %246
  %248 = icmp eq i128 %231, %247
  %249 = getelementptr inbounds i8, ptr %13, i64 5604
  %250 = load i32, ptr %249, align 4
  %251 = icmp sgt i32 %250, %97
  %252 = select i1 %248, i1 %251, i1 false
  %253 = select i1 %252, i32 1, i32 2
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %7, i8 0, i64 5604, i1 false), !alias.scope !112
  %254 = getelementptr inbounds i8, ptr %7, i64 4
  store i32 1, ptr %7, align 4, !tbaa !34, !alias.scope !112
  store i32 %253, ptr %254, align 4, !tbaa !24, !alias.scope !112
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %8) #10
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %8, i8 0, i64 5604, i1 false), !alias.scope !115
  %255 = getelementptr inbounds i8, ptr %8, i64 4
  store i32 1, ptr %8, align 4, !tbaa !34, !alias.scope !115
  store i32 2, ptr %255, align 4, !tbaa !24, !alias.scope !115
  %256 = load i32, ptr %249, align 4, !tbaa !35
  %257 = icmp sgt i32 %256, -1
  br i1 %257, label %258, label %388

258:                                              ; preds = %244
  %259 = load i32, ptr %5, align 4, !tbaa !34
  %260 = icmp ne i32 %259, 0
  %261 = icmp ne i32 %256, 0
  %262 = and i1 %261, %260
  br i1 %262, label %263, label %315

263:                                              ; preds = %258
  %264 = lshr i32 %256, 5
  %265 = and i32 %256, 31
  %266 = add nsw i32 %259, %264
  %267 = icmp sgt i32 %266, 1399
  br i1 %267, label %272, label %268

268:                                              ; preds = %263
  %269 = getelementptr inbounds i8, ptr %5, i64 4
  %270 = sext i32 %259 to i64
  %271 = zext i32 %264 to i64
  br label %275

272:                                              ; preds = %263
  tail call void @llvm.trap()
  unreachable

273:                                              ; preds = %275
  %274 = icmp sgt i32 %256, 31
  br i1 %274, label %290, label %283

275:                                              ; preds = %275, %268
  %276 = phi i64 [ %270, %268 ], [ %277, %275 ]
  %277 = add nsw i64 %276, -1
  %278 = getelementptr inbounds [1400 x i32], ptr %269, i64 0, i64 %277
  %279 = load i32, ptr %278, align 4, !tbaa !24
  %280 = add nsw i64 %277, %271
  %281 = getelementptr inbounds [1400 x i32], ptr %269, i64 0, i64 %280
  store i32 %279, ptr %281, align 4, !tbaa !24
  %282 = icmp eq i64 %277, 0
  br i1 %282, label %273, label %275
283:                                              ; preds = %290, %273
  %284 = load i32, ptr %5, align 4, !tbaa !34
  %285 = add nsw i32 %284, %264
  store i32 %285, ptr %5, align 4, !tbaa !34
  %286 = icmp sgt i32 %284, 0
  br i1 %286, label %287, label %295

287:                                              ; preds = %283
  %288 = zext i32 %265 to i64
  %289 = zext i32 %285 to i64
  br label %298

290:                                              ; preds = %273, %290
  %291 = phi i64 [ %293, %290 ], [ 0, %273 ]
  %292 = getelementptr inbounds [1400 x i32], ptr %269, i64 0, i64 %291
  store i32 0, ptr %292, align 4, !tbaa !24
  %293 = add nuw nsw i64 %291, 1
  %294 = icmp eq i64 %293, %271
  br i1 %294, label %283, label %290
295:                                              ; preds = %298, %283
  %296 = phi i64 [ 0, %283 ], [ %307, %298 ]
  %297 = icmp eq i64 %296, 0
  br i1 %297, label %315, label %310

298:                                              ; preds = %298, %287
  %299 = phi i64 [ %271, %287 ], [ %308, %298 ]
  %300 = phi i64 [ 0, %287 ], [ %307, %298 ]
  %301 = getelementptr inbounds [1400 x i32], ptr %269, i64 0, i64 %299
  %302 = load i32, ptr %301, align 4, !tbaa !24
  %303 = zext i32 %302 to i64
  %304 = shl nuw nsw i64 %303, %288
  %305 = or i64 %304, %300
  %306 = trunc i64 %305 to i32
  store i32 %306, ptr %301, align 4, !tbaa !24
  %307 = lshr i64 %304, 32
  %308 = add nuw nsw i64 %299, 1
  %309 = icmp samesign ult i64 %308, %289
  br i1 %309, label %298, label %295
310:                                              ; preds = %295
  %311 = trunc i64 %296 to i32
  %312 = add nsw i32 %285, 1
  store i32 %312, ptr %5, align 4, !tbaa !34
  %313 = sext i32 %285 to i64
  %314 = getelementptr inbounds [1400 x i32], ptr %269, i64 0, i64 %313
  store i32 %311, ptr %314, align 4, !tbaa !24
  br label %315

315:                                              ; preds = %310, %295, %258
  br i1 %261, label %316, label %435

316:                                              ; preds = %315
  %317 = lshr i32 %256, 5
  %318 = and i32 %256, 31
  %319 = icmp ugt i32 %256, 44767
  br i1 %319, label %325, label %320

320:                                              ; preds = %316
  %321 = zext i32 %317 to i64
  %322 = load i32, ptr %254, align 4, !tbaa !24
  %323 = getelementptr inbounds [1400 x i32], ptr %254, i64 0, i64 %321
  store i32 %322, ptr %323, align 4, !tbaa !24
  %324 = icmp sgt i32 %256, 31
  br i1 %324, label %333, label %326

325:                                              ; preds = %316
  tail call void @llvm.trap()
  unreachable

326:                                              ; preds = %333, %320
  %327 = load i32, ptr %7, align 4, !tbaa !34
  %328 = add nsw i32 %327, %317
  store i32 %328, ptr %7, align 4, !tbaa !34
  %329 = icmp sgt i32 %327, 0
  br i1 %329, label %330, label %338

330:                                              ; preds = %326
  %331 = zext i32 %318 to i64
  %332 = zext i32 %328 to i64
  br label %341

333:                                              ; preds = %320, %333
  %334 = phi i64 [ %336, %333 ], [ 0, %320 ]
  %335 = getelementptr inbounds [1400 x i32], ptr %254, i64 0, i64 %334
  store i32 0, ptr %335, align 4, !tbaa !24
  %336 = add nuw nsw i64 %334, 1
  %337 = icmp eq i64 %336, %321
  br i1 %337, label %326, label %333
338:                                              ; preds = %341, %326
  %339 = phi i64 [ 0, %326 ], [ %350, %341 ]
  %340 = icmp eq i64 %339, 0
  br i1 %340, label %358, label %353

341:                                              ; preds = %341, %330
  %342 = phi i64 [ %321, %330 ], [ %351, %341 ]
  %343 = phi i64 [ 0, %330 ], [ %350, %341 ]
  %344 = getelementptr inbounds [1400 x i32], ptr %254, i64 0, i64 %342
  %345 = load i32, ptr %344, align 4, !tbaa !24
  %346 = zext i32 %345 to i64
  %347 = shl nuw nsw i64 %346, %331
  %348 = or i64 %347, %343
  %349 = trunc i64 %348 to i32
  store i32 %349, ptr %344, align 4, !tbaa !24
  %350 = lshr i64 %347, 32
  %351 = add nuw nsw i64 %342, 1
  %352 = icmp samesign ult i64 %351, %332
  br i1 %352, label %341, label %338
353:                                              ; preds = %338
  %354 = trunc i64 %339 to i32
  %355 = add nsw i32 %328, 1
  store i32 %355, ptr %7, align 4, !tbaa !34
  %356 = sext i32 %328 to i64
  %357 = getelementptr inbounds [1400 x i32], ptr %254, i64 0, i64 %356
  store i32 %354, ptr %357, align 4, !tbaa !24
  br label %358

358:                                              ; preds = %338, %353
  %359 = load i32, ptr %255, align 4, !tbaa !24
  %360 = getelementptr inbounds [1400 x i32], ptr %255, i64 0, i64 %321
  store i32 %359, ptr %360, align 4, !tbaa !24
  br i1 %324, label %368, label %361

361:                                              ; preds = %368, %358
  %362 = load i32, ptr %8, align 4, !tbaa !34
  %363 = add nsw i32 %362, %317
  store i32 %363, ptr %8, align 4, !tbaa !34
  %364 = icmp sgt i32 %362, 0
  br i1 %364, label %365, label %373

365:                                              ; preds = %361
  %366 = zext i32 %318 to i64
  %367 = zext i32 %363 to i64
  br label %376

368:                                              ; preds = %358, %368
  %369 = phi i64 [ %371, %368 ], [ 0, %358 ]
  %370 = getelementptr inbounds [1400 x i32], ptr %255, i64 0, i64 %369
  store i32 0, ptr %370, align 4, !tbaa !24
  %371 = add nuw nsw i64 %369, 1
  %372 = icmp eq i64 %371, %321
  br i1 %372, label %361, label %368
373:                                              ; preds = %376, %361
  %374 = phi i64 [ 0, %361 ], [ %385, %376 ]
  %375 = icmp eq i64 %374, 0
  br i1 %375, label %435, label %426

376:                                              ; preds = %376, %365
  %377 = phi i64 [ %321, %365 ], [ %386, %376 ]
  %378 = phi i64 [ 0, %365 ], [ %385, %376 ]
  %379 = getelementptr inbounds [1400 x i32], ptr %255, i64 0, i64 %377
  %380 = load i32, ptr %379, align 4, !tbaa !24
  %381 = zext i32 %380 to i64
  %382 = shl nuw nsw i64 %381, %366
  %383 = or i64 %382, %378
  %384 = trunc i64 %383 to i32
  store i32 %384, ptr %379, align 4, !tbaa !24
  %385 = lshr i64 %382, 32
  %386 = add nuw nsw i64 %377, 1
  %387 = icmp samesign ult i64 %386, %367
  br i1 %387, label %376, label %373
388:                                              ; preds = %244
  %389 = sub nsw i32 0, %256
  %390 = sdiv i32 %256, -32
  %391 = and i32 %389, 31
  %392 = icmp slt i32 %256, -44767
  br i1 %392, label %398, label %393

393:                                              ; preds = %388
  %394 = zext i32 %390 to i64
  %395 = load i32, ptr %245, align 4, !tbaa !24
  %396 = getelementptr inbounds [1400 x i32], ptr %245, i64 0, i64 %394
  store i32 %395, ptr %396, align 4, !tbaa !24
  %397 = icmp slt i32 %256, -31
  br i1 %397, label %406, label %399

398:                                              ; preds = %388
  tail call void @llvm.trap()
  unreachable

399:                                              ; preds = %406, %393
  %400 = load i32, ptr %6, align 4, !tbaa !34
  %401 = add nsw i32 %400, %390
  store i32 %401, ptr %6, align 4, !tbaa !34
  %402 = icmp sgt i32 %400, 0
  br i1 %402, label %403, label %411

403:                                              ; preds = %399
  %404 = zext i32 %391 to i64
  %405 = zext i32 %401 to i64
  br label %414

406:                                              ; preds = %393, %406
  %407 = phi i64 [ %409, %406 ], [ 0, %393 ]
  %408 = getelementptr inbounds [1400 x i32], ptr %245, i64 0, i64 %407
  store i32 0, ptr %408, align 4, !tbaa !24
  %409 = add nuw nsw i64 %407, 1
  %410 = icmp eq i64 %409, %394
  br i1 %410, label %399, label %406
411:                                              ; preds = %414, %399
  %412 = phi i64 [ 0, %399 ], [ %423, %414 ]
  %413 = icmp eq i64 %412, 0
  br i1 %413, label %435, label %426

414:                                              ; preds = %414, %403
  %415 = phi i64 [ %394, %403 ], [ %424, %414 ]
  %416 = phi i64 [ 0, %403 ], [ %423, %414 ]
  %417 = getelementptr inbounds [1400 x i32], ptr %245, i64 0, i64 %415
  %418 = load i32, ptr %417, align 4, !tbaa !24
  %419 = zext i32 %418 to i64
  %420 = shl nuw nsw i64 %419, %404
  %421 = or i64 %420, %416
  %422 = trunc i64 %421 to i32
  store i32 %422, ptr %417, align 4, !tbaa !24
  %423 = lshr i64 %420, 32
  %424 = add nuw nsw i64 %415, 1
  %425 = icmp samesign ult i64 %424, %405
  br i1 %425, label %414, label %411
426:                                              ; preds = %411, %373
  %427 = phi i64 [ %374, %373 ], [ %412, %411 ]
  %428 = phi i32 [ %363, %373 ], [ %401, %411 ]
  %429 = phi ptr [ %8, %373 ], [ %6, %411 ]
  %430 = getelementptr inbounds i8, ptr %429, i64 4
  %431 = trunc i64 %427 to i32
  %432 = add nsw i32 %428, 1
  store i32 %432, ptr %429, align 4, !tbaa !34
  %433 = sext i32 %428 to i64
  %434 = getelementptr inbounds [1400 x i32], ptr %430, i64 0, i64 %433
  store i32 %431, ptr %434, align 4, !tbaa !24
  br label %435

435:                                              ; preds = %426, %315, %411, %373
  %436 = call fastcc i32 @magnitude(ptr noundef %5, ptr noundef %6, i32 noundef 10) #11
  %437 = icmp sgt i32 %436, -1
  br i1 %437, label %438, label %507

438:                                              ; preds = %435
  %439 = icmp sgt i32 %436, 8
  br i1 %439, label %443, label %440

440:                                              ; preds = %472, %438
  %441 = phi i32 [ %436, %438 ], [ %473, %472 ]
  %442 = icmp sgt i32 %441, 0
  br i1 %442, label %475, label %717

443:                                              ; preds = %438, %472
  %444 = phi i32 [ %473, %472 ], [ %436, %438 ]
  %445 = load i32, ptr %6, align 4, !tbaa !34
  %446 = icmp sgt i32 %445, 0
  br i1 %446, label %447, label %449

447:                                              ; preds = %443
  %448 = zext i32 %445 to i64
  br label %452

449:                                              ; preds = %452, %443
  %450 = phi i64 [ 0, %443 ], [ %461, %452 ]
  %451 = icmp eq i64 %450, 0
  br i1 %451, label %472, label %464

452:                                              ; preds = %452, %447
  %453 = phi i64 [ 0, %447 ], [ %462, %452 ]
  %454 = phi i64 [ 0, %447 ], [ %461, %452 ]
  %455 = getelementptr inbounds [1400 x i32], ptr %245, i64 0, i64 %453
  %456 = load i32, ptr %455, align 4, !tbaa !24
  %457 = zext i32 %456 to i64
  %458 = mul nuw nsw i64 %457, 1000000000
  %459 = add nuw nsw i64 %458, %454
  %460 = trunc i64 %459 to i32
  store i32 %460, ptr %455, align 4, !tbaa !24
  %461 = lshr i64 %459, 32
  %462 = add nuw nsw i64 %453, 1
  %463 = icmp eq i64 %462, %448
  br i1 %463, label %449, label %452
464:                                              ; preds = %449
  %465 = icmp eq i32 %445, 1400
  br i1 %465, label %466, label %467

466:                                              ; preds = %464
  tail call void @llvm.trap()
  unreachable

467:                                              ; preds = %464
  %468 = trunc i64 %450 to i32
  %469 = add nsw i32 %445, 1
  store i32 %469, ptr %6, align 4, !tbaa !34
  %470 = sext i32 %445 to i64
  %471 = getelementptr inbounds [1400 x i32], ptr %245, i64 0, i64 %470
  store i32 %468, ptr %471, align 4, !tbaa !24
  br label %472

472:                                              ; preds = %467, %449
  %473 = add nsw i32 %444, -9
  %474 = icmp sgt i32 %444, 17
  br i1 %474, label %443, label %440
475:                                              ; preds = %440, %505
  %476 = phi i32 [ %477, %505 ], [ %441, %440 ]
  %477 = add nsw i32 %476, -1
  %478 = load i32, ptr %6, align 4, !tbaa !34
  %479 = icmp sgt i32 %478, 0
  br i1 %479, label %480, label %482

480:                                              ; preds = %475
  %481 = zext i32 %478 to i64
  br label %485

482:                                              ; preds = %485, %475
  %483 = phi i64 [ 0, %475 ], [ %494, %485 ]
  %484 = icmp eq i64 %483, 0
  br i1 %484, label %505, label %497

485:                                              ; preds = %485, %480
  %486 = phi i64 [ 0, %480 ], [ %495, %485 ]
  %487 = phi i64 [ 0, %480 ], [ %494, %485 ]
  %488 = getelementptr inbounds [1400 x i32], ptr %245, i64 0, i64 %486
  %489 = load i32, ptr %488, align 4, !tbaa !24
  %490 = zext i32 %489 to i64
  %491 = mul nuw nsw i64 %490, 10
  %492 = add nuw nsw i64 %491, %487
  %493 = trunc i64 %492 to i32
  store i32 %493, ptr %488, align 4, !tbaa !24
  %494 = lshr i64 %492, 32
  %495 = add nuw nsw i64 %486, 1
  %496 = icmp eq i64 %495, %481
  br i1 %496, label %482, label %485
497:                                              ; preds = %482
  %498 = icmp eq i32 %478, 1400
  br i1 %498, label %499, label %500

499:                                              ; preds = %497
  tail call void @llvm.trap()
  unreachable

500:                                              ; preds = %497
  %501 = trunc i64 %483 to i32
  %502 = add nsw i32 %478, 1
  store i32 %502, ptr %6, align 4, !tbaa !34
  %503 = sext i32 %478 to i64
  %504 = getelementptr inbounds [1400 x i32], ptr %245, i64 0, i64 %503
  store i32 %501, ptr %504, align 4, !tbaa !24
  br label %505

505:                                              ; preds = %500, %482
  %506 = icmp sgt i32 %476, 1
  br i1 %506, label %475, label %717
507:                                              ; preds = %435
  %508 = sub nsw i32 0, %436
  %509 = icmp slt i32 %436, -8
  br i1 %509, label %510, label %512

510:                                              ; preds = %507
  %511 = getelementptr inbounds i8, ptr %5, i64 4
  br label %517

512:                                              ; preds = %546, %507
  %513 = phi i32 [ %508, %507 ], [ %547, %546 ]
  %514 = icmp sgt i32 %513, 0
  br i1 %514, label %515, label %581

515:                                              ; preds = %512
  %516 = getelementptr inbounds i8, ptr %5, i64 4
  br label %549

517:                                              ; preds = %546, %510
  %518 = phi i32 [ %508, %510 ], [ %547, %546 ]
  %519 = load i32, ptr %5, align 4, !tbaa !34
  %520 = icmp sgt i32 %519, 0
  br i1 %520, label %521, label %523

521:                                              ; preds = %517
  %522 = zext i32 %519 to i64
  br label %526

523:                                              ; preds = %526, %517
  %524 = phi i64 [ 0, %517 ], [ %535, %526 ]
  %525 = icmp eq i64 %524, 0
  br i1 %525, label %546, label %538

526:                                              ; preds = %526, %521
  %527 = phi i64 [ 0, %521 ], [ %536, %526 ]
  %528 = phi i64 [ 0, %521 ], [ %535, %526 ]
  %529 = getelementptr inbounds [1400 x i32], ptr %511, i64 0, i64 %527
  %530 = load i32, ptr %529, align 4, !tbaa !24
  %531 = zext i32 %530 to i64
  %532 = mul nuw nsw i64 %531, 1000000000
  %533 = add nuw nsw i64 %532, %528
  %534 = trunc i64 %533 to i32
  store i32 %534, ptr %529, align 4, !tbaa !24
  %535 = lshr i64 %533, 32
  %536 = add nuw nsw i64 %527, 1
  %537 = icmp eq i64 %536, %522
  br i1 %537, label %523, label %526
538:                                              ; preds = %523
  %539 = icmp eq i32 %519, 1400
  br i1 %539, label %540, label %541

540:                                              ; preds = %538
  tail call void @llvm.trap()
  unreachable

541:                                              ; preds = %538
  %542 = trunc i64 %524 to i32
  %543 = add nsw i32 %519, 1
  store i32 %543, ptr %5, align 4, !tbaa !34
  %544 = sext i32 %519 to i64
  %545 = getelementptr inbounds [1400 x i32], ptr %511, i64 0, i64 %544
  store i32 %542, ptr %545, align 4, !tbaa !24
  br label %546

546:                                              ; preds = %541, %523
  %547 = add nsw i32 %518, -9
  %548 = icmp sgt i32 %518, 17
  br i1 %548, label %517, label %512
549:                                              ; preds = %579, %515
  %550 = phi i32 [ %513, %515 ], [ %551, %579 ]
  %551 = add nsw i32 %550, -1
  %552 = load i32, ptr %5, align 4, !tbaa !34
  %553 = icmp sgt i32 %552, 0
  br i1 %553, label %554, label %556

554:                                              ; preds = %549
  %555 = zext i32 %552 to i64
  br label %559

556:                                              ; preds = %559, %549
  %557 = phi i64 [ 0, %549 ], [ %568, %559 ]
  %558 = icmp eq i64 %557, 0
  br i1 %558, label %579, label %571

559:                                              ; preds = %559, %554
  %560 = phi i64 [ 0, %554 ], [ %569, %559 ]
  %561 = phi i64 [ 0, %554 ], [ %568, %559 ]
  %562 = getelementptr inbounds [1400 x i32], ptr %516, i64 0, i64 %560
  %563 = load i32, ptr %562, align 4, !tbaa !24
  %564 = zext i32 %563 to i64
  %565 = mul nuw nsw i64 %564, 10
  %566 = add nuw nsw i64 %565, %561
  %567 = trunc i64 %566 to i32
  store i32 %567, ptr %562, align 4, !tbaa !24
  %568 = lshr i64 %566, 32
  %569 = add nuw nsw i64 %560, 1
  %570 = icmp eq i64 %569, %555
  br i1 %570, label %556, label %559
571:                                              ; preds = %556
  %572 = icmp eq i32 %552, 1400
  br i1 %572, label %573, label %574

573:                                              ; preds = %571
  tail call void @llvm.trap()
  unreachable

574:                                              ; preds = %571
  %575 = trunc i64 %557 to i32
  %576 = add nsw i32 %552, 1
  store i32 %576, ptr %5, align 4, !tbaa !34
  %577 = sext i32 %552 to i64
  %578 = getelementptr inbounds [1400 x i32], ptr %516, i64 0, i64 %577
  store i32 %575, ptr %578, align 4, !tbaa !24
  br label %579

579:                                              ; preds = %574, %556
  %580 = icmp sgt i32 %550, 1
  br i1 %580, label %549, label %581
581:                                              ; preds = %579, %512
  br i1 %509, label %585, label %582

582:                                              ; preds = %614, %581
  %583 = phi i32 [ %508, %581 ], [ %615, %614 ]
  %584 = icmp sgt i32 %583, 0
  br i1 %584, label %617, label %649

585:                                              ; preds = %581, %614
  %586 = phi i32 [ %615, %614 ], [ %508, %581 ]
  %587 = load i32, ptr %7, align 4, !tbaa !34
  %588 = icmp sgt i32 %587, 0
  br i1 %588, label %589, label %591

589:                                              ; preds = %585
  %590 = zext i32 %587 to i64
  br label %594

591:                                              ; preds = %594, %585
  %592 = phi i64 [ 0, %585 ], [ %603, %594 ]
  %593 = icmp eq i64 %592, 0
  br i1 %593, label %614, label %606

594:                                              ; preds = %594, %589
  %595 = phi i64 [ 0, %589 ], [ %604, %594 ]
  %596 = phi i64 [ 0, %589 ], [ %603, %594 ]
  %597 = getelementptr inbounds [1400 x i32], ptr %254, i64 0, i64 %595
  %598 = load i32, ptr %597, align 4, !tbaa !24
  %599 = zext i32 %598 to i64
  %600 = mul nuw nsw i64 %599, 1000000000
  %601 = add nuw nsw i64 %600, %596
  %602 = trunc i64 %601 to i32
  store i32 %602, ptr %597, align 4, !tbaa !24
  %603 = lshr i64 %601, 32
  %604 = add nuw nsw i64 %595, 1
  %605 = icmp eq i64 %604, %590
  br i1 %605, label %591, label %594
606:                                              ; preds = %591
  %607 = icmp eq i32 %587, 1400
  br i1 %607, label %608, label %609

608:                                              ; preds = %606
  tail call void @llvm.trap()
  unreachable

609:                                              ; preds = %606
  %610 = trunc i64 %592 to i32
  %611 = add nsw i32 %587, 1
  store i32 %611, ptr %7, align 4, !tbaa !34
  %612 = sext i32 %587 to i64
  %613 = getelementptr inbounds [1400 x i32], ptr %254, i64 0, i64 %612
  store i32 %610, ptr %613, align 4, !tbaa !24
  br label %614

614:                                              ; preds = %609, %591
  %615 = add nsw i32 %586, -9
  %616 = icmp sgt i32 %586, 17
  br i1 %616, label %585, label %582
617:                                              ; preds = %582, %647
  %618 = phi i32 [ %619, %647 ], [ %583, %582 ]
  %619 = add nsw i32 %618, -1
  %620 = load i32, ptr %7, align 4, !tbaa !34
  %621 = icmp sgt i32 %620, 0
  br i1 %621, label %622, label %624

622:                                              ; preds = %617
  %623 = zext i32 %620 to i64
  br label %627

624:                                              ; preds = %627, %617
  %625 = phi i64 [ 0, %617 ], [ %636, %627 ]
  %626 = icmp eq i64 %625, 0
  br i1 %626, label %647, label %639

627:                                              ; preds = %627, %622
  %628 = phi i64 [ 0, %622 ], [ %637, %627 ]
  %629 = phi i64 [ 0, %622 ], [ %636, %627 ]
  %630 = getelementptr inbounds [1400 x i32], ptr %254, i64 0, i64 %628
  %631 = load i32, ptr %630, align 4, !tbaa !24
  %632 = zext i32 %631 to i64
  %633 = mul nuw nsw i64 %632, 10
  %634 = add nuw nsw i64 %633, %629
  %635 = trunc i64 %634 to i32
  store i32 %635, ptr %630, align 4, !tbaa !24
  %636 = lshr i64 %634, 32
  %637 = add nuw nsw i64 %628, 1
  %638 = icmp eq i64 %637, %623
  br i1 %638, label %624, label %627
639:                                              ; preds = %624
  %640 = icmp eq i32 %620, 1400
  br i1 %640, label %641, label %642

641:                                              ; preds = %639
  tail call void @llvm.trap()
  unreachable

642:                                              ; preds = %639
  %643 = trunc i64 %625 to i32
  %644 = add nsw i32 %620, 1
  store i32 %644, ptr %7, align 4, !tbaa !34
  %645 = sext i32 %620 to i64
  %646 = getelementptr inbounds [1400 x i32], ptr %254, i64 0, i64 %645
  store i32 %643, ptr %646, align 4, !tbaa !24
  br label %647

647:                                              ; preds = %642, %624
  %648 = icmp sgt i32 %618, 1
  br i1 %648, label %617, label %649
649:                                              ; preds = %647, %582
  br i1 %509, label %653, label %650

650:                                              ; preds = %682, %649
  %651 = phi i32 [ %508, %649 ], [ %683, %682 ]
  %652 = icmp sgt i32 %651, 0
  br i1 %652, label %685, label %717

653:                                              ; preds = %649, %682
  %654 = phi i32 [ %683, %682 ], [ %508, %649 ]
  %655 = load i32, ptr %8, align 4, !tbaa !34
  %656 = icmp sgt i32 %655, 0
  br i1 %656, label %657, label %659

657:                                              ; preds = %653
  %658 = zext i32 %655 to i64
  br label %662

659:                                              ; preds = %662, %653
  %660 = phi i64 [ 0, %653 ], [ %671, %662 ]
  %661 = icmp eq i64 %660, 0
  br i1 %661, label %682, label %674

662:                                              ; preds = %662, %657
  %663 = phi i64 [ 0, %657 ], [ %672, %662 ]
  %664 = phi i64 [ 0, %657 ], [ %671, %662 ]
  %665 = getelementptr inbounds [1400 x i32], ptr %255, i64 0, i64 %663
  %666 = load i32, ptr %665, align 4, !tbaa !24
  %667 = zext i32 %666 to i64
  %668 = mul nuw nsw i64 %667, 1000000000
  %669 = add nuw nsw i64 %668, %664
  %670 = trunc i64 %669 to i32
  store i32 %670, ptr %665, align 4, !tbaa !24
  %671 = lshr i64 %669, 32
  %672 = add nuw nsw i64 %663, 1
  %673 = icmp eq i64 %672, %658
  br i1 %673, label %659, label %662
674:                                              ; preds = %659
  %675 = icmp eq i32 %655, 1400
  br i1 %675, label %676, label %677

676:                                              ; preds = %674
  tail call void @llvm.trap()
  unreachable

677:                                              ; preds = %674
  %678 = trunc i64 %660 to i32
  %679 = add nsw i32 %655, 1
  store i32 %679, ptr %8, align 4, !tbaa !34
  %680 = sext i32 %655 to i64
  %681 = getelementptr inbounds [1400 x i32], ptr %255, i64 0, i64 %680
  store i32 %678, ptr %681, align 4, !tbaa !24
  br label %682

682:                                              ; preds = %677, %659
  %683 = add nsw i32 %654, -9
  %684 = icmp sgt i32 %654, 17
  br i1 %684, label %653, label %650
685:                                              ; preds = %650, %715
  %686 = phi i32 [ %687, %715 ], [ %651, %650 ]
  %687 = add nsw i32 %686, -1
  %688 = load i32, ptr %8, align 4, !tbaa !34
  %689 = icmp sgt i32 %688, 0
  br i1 %689, label %690, label %692

690:                                              ; preds = %685
  %691 = zext i32 %688 to i64
  br label %695

692:                                              ; preds = %695, %685
  %693 = phi i64 [ 0, %685 ], [ %704, %695 ]
  %694 = icmp eq i64 %693, 0
  br i1 %694, label %715, label %707

695:                                              ; preds = %695, %690
  %696 = phi i64 [ 0, %690 ], [ %705, %695 ]
  %697 = phi i64 [ 0, %690 ], [ %704, %695 ]
  %698 = getelementptr inbounds [1400 x i32], ptr %255, i64 0, i64 %696
  %699 = load i32, ptr %698, align 4, !tbaa !24
  %700 = zext i32 %699 to i64
  %701 = mul nuw nsw i64 %700, 10
  %702 = add nuw nsw i64 %701, %697
  %703 = trunc i64 %702 to i32
  store i32 %703, ptr %698, align 4, !tbaa !24
  %704 = lshr i64 %702, 32
  %705 = add nuw nsw i64 %696, 1
  %706 = icmp eq i64 %705, %691
  br i1 %706, label %692, label %695
707:                                              ; preds = %692
  %708 = icmp eq i32 %688, 1400
  br i1 %708, label %709, label %710

709:                                              ; preds = %707
  tail call void @llvm.trap()
  unreachable

710:                                              ; preds = %707
  %711 = trunc i64 %693 to i32
  %712 = add nsw i32 %688, 1
  store i32 %712, ptr %8, align 4, !tbaa !34
  %713 = sext i32 %688 to i64
  %714 = getelementptr inbounds [1400 x i32], ptr %255, i64 0, i64 %713
  store i32 %711, ptr %714, align 4, !tbaa !24
  br label %715

715:                                              ; preds = %710, %692
  %716 = icmp sgt i32 %686, 1
  br i1 %716, label %685, label %717
717:                                              ; preds = %715, %505, %650, %440
  %718 = load i32, ptr %6, align 4, !tbaa !34
  %719 = getelementptr inbounds i8, ptr %5, i64 4
  %720 = sext i32 %718 to i64
  %721 = getelementptr inbounds i8, ptr %9, i64 4
  %722 = getelementptr inbounds i8, ptr %10, i64 4
  %723 = add i32 %436, 1
  %724 = load i32, ptr %249, align 4
  br label %725

725:                                              ; preds = %1060, %717
  %726 = phi i32 [ %724, %717 ], [ %1061, %1060 ]
  %727 = phi i32 [ 1, %717 ], [ %1063, %1060 ]
  %728 = phi i128 [ 0, %717 ], [ %1062, %1060 ]
  %729 = load i32, ptr %5, align 4, !tbaa !34
  br label %730

730:                                              ; preds = %796, %725
  %731 = phi i32 [ %729, %725 ], [ %797, %796 ]
  %732 = phi i32 [ 0, %725 ], [ %798, %796 ]
  %733 = icmp eq i32 %731, %718
  br i1 %733, label %734, label %738

734:                                              ; preds = %730
  %735 = icmp eq i32 %731, 0
  br i1 %735, label %754, label %736

736:                                              ; preds = %734
  %737 = sext i32 %731 to i64
  br label %743

738:                                              ; preds = %730
  %739 = icmp slt i32 %731, %718
  %740 = select i1 %739, i32 -1, i32 1
  br label %754

741:                                              ; preds = %743
  %742 = icmp eq i64 %745, 0
  br i1 %742, label %754, label %743
743:                                              ; preds = %736, %741
  %744 = phi i64 [ %737, %736 ], [ %745, %741 ]
  %745 = add nsw i64 %744, -1
  %746 = getelementptr inbounds [1400 x i32], ptr %719, i64 0, i64 %745
  %747 = load i32, ptr %746, align 4, !tbaa !24
  %748 = getelementptr inbounds [1400 x i32], ptr %245, i64 0, i64 %745
  %749 = load i32, ptr %748, align 4, !tbaa !24
  %750 = icmp eq i32 %747, %749
  br i1 %750, label %741, label %751
751:                                              ; preds = %743
  %752 = icmp ult i32 %747, %749
  %753 = select i1 %752, i32 -1, i32 1
  br label %754

754:                                              ; preds = %741, %734, %751, %738
  %755 = phi i32 [ %740, %738 ], [ %753, %751 ], [ 0, %734 ], [ 0, %741 ]
  %756 = icmp sgt i32 %755, -1
  br i1 %756, label %757, label %799

757:                                              ; preds = %754
  %758 = icmp sgt i32 %731, 0
  br i1 %758, label %759, label %761

759:                                              ; preds = %757
  %760 = zext i32 %731 to i64
  br label %774

761:                                              ; preds = %782, %757
  %762 = icmp eq i32 %731, 0
  br i1 %762, label %796, label %763

763:                                              ; preds = %761
  %764 = sext i32 %731 to i64
  br label %765

765:                                              ; preds = %771, %763
  %766 = phi i64 [ %764, %763 ], [ %767, %771 ]
  %767 = add nsw i64 %766, -1
  %768 = getelementptr inbounds [1400 x i32], ptr %719, i64 0, i64 %767
  %769 = load i32, ptr %768, align 4, !tbaa !24
  %770 = icmp eq i32 %769, 0
  br i1 %770, label %771, label %794

771:                                              ; preds = %765
  %772 = trunc i64 %767 to i32
  store i32 %772, ptr %5, align 4, !tbaa !34
  %773 = icmp eq i64 %767, 0
  br i1 %773, label %796, label %765
774:                                              ; preds = %782, %759
  %775 = phi i64 [ 0, %759 ], [ %792, %782 ]
  %776 = phi i64 [ 0, %759 ], [ %791, %782 ]
  %777 = icmp slt i64 %775, %720
  br i1 %777, label %778, label %782

778:                                              ; preds = %774
  %779 = getelementptr inbounds [1400 x i32], ptr %245, i64 0, i64 %775
  %780 = load i32, ptr %779, align 4, !tbaa !24
  %781 = zext i32 %780 to i64
  br label %782

782:                                              ; preds = %778, %774
  %783 = phi i64 [ %781, %778 ], [ 0, %774 ]
  %784 = add nuw nsw i64 %783, %776
  %785 = getelementptr inbounds [1400 x i32], ptr %719, i64 0, i64 %775
  %786 = load i32, ptr %785, align 4, !tbaa !24
  %787 = zext i32 %786 to i64
  %788 = trunc i64 %784 to i32
  %789 = sub i32 %786, %788
  store i32 %789, ptr %785, align 4, !tbaa !24
  %790 = icmp samesign ugt i64 %784, %787
  %791 = zext i1 %790 to i64
  %792 = add nuw nsw i64 %775, 1
  %793 = icmp eq i64 %792, %760
  br i1 %793, label %761, label %774
794:                                              ; preds = %765
  %795 = trunc i64 %766 to i32
  br label %796

796:                                              ; preds = %771, %794, %761
  %797 = phi i32 [ %731, %761 ], [ %795, %794 ], [ 0, %771 ]
  %798 = add i32 %732, 1
  br label %730
799:                                              ; preds = %754
  %800 = mul i128 %728, 10
  %801 = zext i32 %732 to i128
  %802 = add i128 %800, %801
  %803 = load i32, ptr %7, align 4, !tbaa !34
  %804 = icmp eq i32 %731, %803
  br i1 %804, label %805, label %809

805:                                              ; preds = %799
  %806 = icmp eq i32 %731, 0
  br i1 %806, label %825, label %807

807:                                              ; preds = %805
  %808 = sext i32 %731 to i64
  br label %814

809:                                              ; preds = %799
  %810 = icmp slt i32 %731, %803
  %811 = select i1 %810, i32 -1, i32 1
  br label %825

812:                                              ; preds = %814
  %813 = icmp eq i64 %816, 0
  br i1 %813, label %825, label %814
814:                                              ; preds = %807, %812
  %815 = phi i64 [ %808, %807 ], [ %816, %812 ]
  %816 = add nsw i64 %815, -1
  %817 = getelementptr inbounds [1400 x i32], ptr %719, i64 0, i64 %816
  %818 = load i32, ptr %817, align 4, !tbaa !24
  %819 = getelementptr inbounds [1400 x i32], ptr %254, i64 0, i64 %816
  %820 = load i32, ptr %819, align 4, !tbaa !24
  %821 = icmp eq i32 %818, %820
  br i1 %821, label %812, label %822
822:                                              ; preds = %814
  %823 = icmp ult i32 %818, %820
  %824 = select i1 %823, i32 -1, i32 1
  br label %825

825:                                              ; preds = %812, %805, %822, %809
  %826 = phi i32 [ %811, %809 ], [ %824, %822 ], [ 0, %805 ], [ 0, %812 ]
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %9) #10
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %9, ptr noundef nonnull align 4 dereferenceable(5604) %6, i64 5604, i1 false), !tbaa.struct !30
  %827 = load i32, ptr %9, align 4, !tbaa !34
  %828 = icmp sgt i32 %827, 0
  br i1 %828, label %829, label %832

829:                                              ; preds = %825
  %830 = zext i32 %827 to i64
  %831 = sext i32 %731 to i64
  br label %845

832:                                              ; preds = %853, %825
  %833 = icmp eq i32 %827, 0
  br i1 %833, label %865, label %834

834:                                              ; preds = %832
  %835 = sext i32 %827 to i64
  br label %836

836:                                              ; preds = %842, %834
  %837 = phi i64 [ %835, %834 ], [ %838, %842 ]
  %838 = add nsw i64 %837, -1
  %839 = getelementptr inbounds [1400 x i32], ptr %721, i64 0, i64 %838
  %840 = load i32, ptr %839, align 4, !tbaa !24
  %841 = icmp eq i32 %840, 0
  br i1 %841, label %842, label %865

842:                                              ; preds = %836
  %843 = trunc i64 %838 to i32
  store i32 %843, ptr %9, align 4, !tbaa !34
  %844 = icmp eq i64 %838, 0
  br i1 %844, label %865, label %836
845:                                              ; preds = %853, %829
  %846 = phi i64 [ 0, %829 ], [ %863, %853 ]
  %847 = phi i64 [ 0, %829 ], [ %862, %853 ]
  %848 = icmp slt i64 %846, %831
  br i1 %848, label %849, label %853

849:                                              ; preds = %845
  %850 = getelementptr inbounds [1400 x i32], ptr %719, i64 0, i64 %846
  %851 = load i32, ptr %850, align 4, !tbaa !24
  %852 = zext i32 %851 to i64
  br label %853

853:                                              ; preds = %849, %845
  %854 = phi i64 [ %852, %849 ], [ 0, %845 ]
  %855 = add nuw nsw i64 %854, %847
  %856 = getelementptr inbounds [1400 x i32], ptr %721, i64 0, i64 %846
  %857 = load i32, ptr %856, align 4, !tbaa !24
  %858 = zext i32 %857 to i64
  %859 = trunc i64 %855 to i32
  %860 = sub i32 %857, %859
  store i32 %860, ptr %856, align 4, !tbaa !24
  %861 = icmp samesign ugt i64 %855, %858
  %862 = zext i1 %861 to i64
  %863 = add nuw nsw i64 %846, 1
  %864 = icmp eq i64 %863, %830
  br i1 %864, label %832, label %845
865:                                              ; preds = %842, %836, %832
  %866 = load i32, ptr %9, align 4, !tbaa !34
  %867 = load i32, ptr %8, align 4, !tbaa !34
  %868 = icmp eq i32 %866, %867
  br i1 %868, label %869, label %873

869:                                              ; preds = %865
  %870 = icmp eq i32 %866, 0
  br i1 %870, label %889, label %871

871:                                              ; preds = %869
  %872 = sext i32 %866 to i64
  br label %878

873:                                              ; preds = %865
  %874 = icmp slt i32 %866, %867
  %875 = select i1 %874, i32 -1, i32 1
  br label %889

876:                                              ; preds = %878
  %877 = icmp eq i64 %880, 0
  br i1 %877, label %889, label %878
878:                                              ; preds = %871, %876
  %879 = phi i64 [ %872, %871 ], [ %880, %876 ]
  %880 = add nsw i64 %879, -1
  %881 = getelementptr inbounds [1400 x i32], ptr %721, i64 0, i64 %880
  %882 = load i32, ptr %881, align 4, !tbaa !24
  %883 = getelementptr inbounds [1400 x i32], ptr %255, i64 0, i64 %880
  %884 = load i32, ptr %883, align 4, !tbaa !24
  %885 = icmp eq i32 %882, %884
  br i1 %885, label %876, label %886
886:                                              ; preds = %878
  %887 = icmp ult i32 %882, %884
  %888 = select i1 %887, i32 -1, i32 1
  br label %889

889:                                              ; preds = %876, %869, %886, %873
  %890 = phi i32 [ %875, %873 ], [ %888, %886 ], [ 0, %869 ], [ 0, %876 ]
  %891 = icmp slt i32 %826, 0
  br i1 %891, label %895, label %892

892:                                              ; preds = %889
  %893 = icmp eq i32 %826, 0
  %894 = and i1 %220, %893
  br label %895

895:                                              ; preds = %892, %889
  %896 = phi i1 [ true, %889 ], [ %894, %892 ]
  %897 = icmp slt i32 %890, 0
  br i1 %897, label %901, label %898

898:                                              ; preds = %895
  %899 = icmp eq i32 %890, 0
  %900 = and i1 %220, %899
  br label %901

901:                                              ; preds = %898, %895
  %902 = phi i1 [ true, %895 ], [ %900, %898 ]
  %903 = or i1 %896, %902
  %904 = icmp sgt i32 %731, 0
  br i1 %903, label %905, label %980

905:                                              ; preds = %901
  br i1 %904, label %906, label %908

906:                                              ; preds = %905
  %907 = zext i32 %731 to i64
  br label %911

908:                                              ; preds = %911, %905
  %909 = phi i64 [ 0, %905 ], [ %920, %911 ]
  %910 = icmp eq i64 %909, 0
  br i1 %910, label %931, label %923

911:                                              ; preds = %911, %906
  %912 = phi i64 [ 0, %906 ], [ %921, %911 ]
  %913 = phi i64 [ 0, %906 ], [ %920, %911 ]
  %914 = getelementptr inbounds [1400 x i32], ptr %719, i64 0, i64 %912
  %915 = load i32, ptr %914, align 4, !tbaa !24
  %916 = zext i32 %915 to i64
  %917 = shl nuw nsw i64 %916, 1
  %918 = add nuw nsw i64 %917, %913
  %919 = trunc i64 %918 to i32
  store i32 %919, ptr %914, align 4, !tbaa !24
  %920 = lshr i64 %918, 32
  %921 = add nuw nsw i64 %912, 1
  %922 = icmp eq i64 %921, %907
  br i1 %922, label %908, label %911
923:                                              ; preds = %908
  %924 = icmp eq i32 %731, 1400
  br i1 %924, label %925, label %926

925:                                              ; preds = %923
  store i32 %726, ptr %249, align 4
  tail call void @llvm.trap()
  unreachable

926:                                              ; preds = %923
  %927 = trunc i64 %909 to i32
  %928 = add nsw i32 %731, 1
  store i32 %928, ptr %5, align 4, !tbaa !34
  %929 = sext i32 %731 to i64
  %930 = getelementptr inbounds [1400 x i32], ptr %719, i64 0, i64 %929
  store i32 %927, ptr %930, align 4, !tbaa !24
  br label %931

931:                                              ; preds = %926, %908
  %932 = load i32, ptr %5, align 4, !tbaa !34
  %933 = icmp eq i32 %932, %718
  br i1 %933, label %934, label %938

934:                                              ; preds = %931
  %935 = icmp eq i32 %932, 0
  br i1 %935, label %954, label %936

936:                                              ; preds = %934
  %937 = sext i32 %932 to i64
  br label %943

938:                                              ; preds = %931
  %939 = icmp slt i32 %932, %718
  %940 = select i1 %939, i32 -1, i32 1
  br label %954

941:                                              ; preds = %943
  %942 = icmp eq i64 %945, 0
  br i1 %942, label %954, label %943
943:                                              ; preds = %936, %941
  %944 = phi i64 [ %937, %936 ], [ %945, %941 ]
  %945 = add nsw i64 %944, -1
  %946 = getelementptr inbounds [1400 x i32], ptr %719, i64 0, i64 %945
  %947 = load i32, ptr %946, align 4, !tbaa !24
  %948 = getelementptr inbounds [1400 x i32], ptr %245, i64 0, i64 %945
  %949 = load i32, ptr %948, align 4, !tbaa !24
  %950 = icmp eq i32 %947, %949
  br i1 %950, label %941, label %951
951:                                              ; preds = %943
  %952 = icmp ult i32 %947, %949
  %953 = select i1 %952, i32 -1, i32 1
  br label %954

954:                                              ; preds = %941, %934, %951, %938
  %955 = phi i32 [ %940, %938 ], [ %953, %951 ], [ 0, %934 ], [ 0, %941 ]
  br i1 %902, label %956, label %966

956:                                              ; preds = %954
  %957 = icmp slt i32 %955, 1
  %958 = and i1 %896, %957
  br i1 %958, label %959, label %964

959:                                              ; preds = %956
  %960 = icmp ne i32 %955, 0
  %961 = and i128 %801, 1
  %962 = icmp eq i128 %961, 0
  %963 = select i1 %960, i1 true, i1 %962
  br i1 %963, label %966, label %964

964:                                              ; preds = %959, %956
  %965 = add i128 %802, 1
  br label %966

966:                                              ; preds = %964, %959, %954
  %967 = phi i128 [ %965, %964 ], [ %802, %959 ], [ %802, %954 ]
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %10) #10
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %10, i8 0, i64 5604, i1 false), !alias.scope !119
  %968 = icmp eq i128 %967, 0
  br i1 %968, label %978, label %969

969:                                              ; preds = %966, %969
  %970 = phi i128 [ %976, %969 ], [ %967, %966 ]
  %971 = trunc i128 %970 to i32
  %972 = load i32, ptr %10, align 4, !tbaa !34, !alias.scope !119
  %973 = add nsw i32 %972, 1
  store i32 %973, ptr %10, align 4, !tbaa !34, !alias.scope !119
  %974 = sext i32 %972 to i64
  %975 = getelementptr inbounds [1400 x i32], ptr %722, i64 0, i64 %974
  store i32 %971, ptr %975, align 4, !tbaa !24, !alias.scope !119
  %976 = lshr i128 %970, 32
  %977 = icmp ult i128 %970, 4294967296
  br i1 %977, label %978, label %969
978:                                              ; preds = %969, %966
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %13, ptr noundef nonnull align 4 dereferenceable(5604) %10, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %10) #10
  %979 = sub i32 %723, %727
  br label %1060

980:                                              ; preds = %901
  br i1 %904, label %981, label %983

981:                                              ; preds = %980
  %982 = zext i32 %731 to i64
  br label %986

983:                                              ; preds = %986, %980
  %984 = phi i64 [ 0, %980 ], [ %995, %986 ]
  %985 = icmp eq i64 %984, 0
  br i1 %985, label %1006, label %998

986:                                              ; preds = %986, %981
  %987 = phi i64 [ 0, %981 ], [ %996, %986 ]
  %988 = phi i64 [ 0, %981 ], [ %995, %986 ]
  %989 = getelementptr inbounds [1400 x i32], ptr %719, i64 0, i64 %987
  %990 = load i32, ptr %989, align 4, !tbaa !24
  %991 = zext i32 %990 to i64
  %992 = mul nuw nsw i64 %991, 10
  %993 = add nuw nsw i64 %992, %988
  %994 = trunc i64 %993 to i32
  store i32 %994, ptr %989, align 4, !tbaa !24
  %995 = lshr i64 %993, 32
  %996 = add nuw nsw i64 %987, 1
  %997 = icmp eq i64 %996, %982
  br i1 %997, label %983, label %986
998:                                              ; preds = %983
  %999 = icmp eq i32 %731, 1400
  br i1 %999, label %1000, label %1001

1000:                                             ; preds = %998
  store i32 %726, ptr %249, align 4
  tail call void @llvm.trap()
  unreachable

1001:                                             ; preds = %998
  %1002 = trunc i64 %984 to i32
  %1003 = add nsw i32 %731, 1
  store i32 %1003, ptr %5, align 4, !tbaa !34
  %1004 = sext i32 %731 to i64
  %1005 = getelementptr inbounds [1400 x i32], ptr %719, i64 0, i64 %1004
  store i32 %1002, ptr %1005, align 4, !tbaa !24
  br label %1006

1006:                                             ; preds = %1001, %983
  %1007 = icmp sgt i32 %803, 0
  br i1 %1007, label %1008, label %1010

1008:                                             ; preds = %1006
  %1009 = zext i32 %803 to i64
  br label %1013

1010:                                             ; preds = %1013, %1006
  %1011 = phi i64 [ 0, %1006 ], [ %1022, %1013 ]
  %1012 = icmp eq i64 %1011, 0
  br i1 %1012, label %1033, label %1025

1013:                                             ; preds = %1013, %1008
  %1014 = phi i64 [ 0, %1008 ], [ %1023, %1013 ]
  %1015 = phi i64 [ 0, %1008 ], [ %1022, %1013 ]
  %1016 = getelementptr inbounds [1400 x i32], ptr %254, i64 0, i64 %1014
  %1017 = load i32, ptr %1016, align 4, !tbaa !24
  %1018 = zext i32 %1017 to i64
  %1019 = mul nuw nsw i64 %1018, 10
  %1020 = add nuw nsw i64 %1019, %1015
  %1021 = trunc i64 %1020 to i32
  store i32 %1021, ptr %1016, align 4, !tbaa !24
  %1022 = lshr i64 %1020, 32
  %1023 = add nuw nsw i64 %1014, 1
  %1024 = icmp eq i64 %1023, %1009
  br i1 %1024, label %1010, label %1013
1025:                                             ; preds = %1010
  %1026 = icmp eq i32 %803, 1400
  br i1 %1026, label %1027, label %1028

1027:                                             ; preds = %1025
  store i32 %726, ptr %249, align 4
  tail call void @llvm.trap()
  unreachable

1028:                                             ; preds = %1025
  %1029 = trunc i64 %1011 to i32
  %1030 = add nsw i32 %803, 1
  store i32 %1030, ptr %7, align 4, !tbaa !34
  %1031 = sext i32 %803 to i64
  %1032 = getelementptr inbounds [1400 x i32], ptr %254, i64 0, i64 %1031
  store i32 %1029, ptr %1032, align 4, !tbaa !24
  br label %1033

1033:                                             ; preds = %1028, %1010
  %1034 = icmp sgt i32 %867, 0
  br i1 %1034, label %1035, label %1037

1035:                                             ; preds = %1033
  %1036 = zext i32 %867 to i64
  br label %1040

1037:                                             ; preds = %1040, %1033
  %1038 = phi i64 [ 0, %1033 ], [ %1049, %1040 ]
  %1039 = icmp eq i64 %1038, 0
  br i1 %1039, label %1060, label %1052

1040:                                             ; preds = %1040, %1035
  %1041 = phi i64 [ 0, %1035 ], [ %1050, %1040 ]
  %1042 = phi i64 [ 0, %1035 ], [ %1049, %1040 ]
  %1043 = getelementptr inbounds [1400 x i32], ptr %255, i64 0, i64 %1041
  %1044 = load i32, ptr %1043, align 4, !tbaa !24
  %1045 = zext i32 %1044 to i64
  %1046 = mul nuw nsw i64 %1045, 10
  %1047 = add nuw nsw i64 %1046, %1042
  %1048 = trunc i64 %1047 to i32
  store i32 %1048, ptr %1043, align 4, !tbaa !24
  %1049 = lshr i64 %1047, 32
  %1050 = add nuw nsw i64 %1041, 1
  %1051 = icmp eq i64 %1050, %1036
  br i1 %1051, label %1037, label %1040
1052:                                             ; preds = %1037
  %1053 = icmp eq i32 %867, 1400
  br i1 %1053, label %1054, label %1055

1054:                                             ; preds = %1052
  store i32 %726, ptr %249, align 4
  tail call void @llvm.trap()
  unreachable

1055:                                             ; preds = %1052
  %1056 = trunc i64 %1038 to i32
  %1057 = add nsw i32 %867, 1
  store i32 %1057, ptr %8, align 4, !tbaa !34
  %1058 = sext i32 %867 to i64
  %1059 = getelementptr inbounds [1400 x i32], ptr %255, i64 0, i64 %1058
  store i32 %1056, ptr %1059, align 4, !tbaa !24
  br label %1060

1060:                                             ; preds = %1055, %1037, %978
  %1061 = phi i32 [ %979, %978 ], [ %726, %1037 ], [ %726, %1055 ]
  %1062 = phi i128 [ %967, %978 ], [ %802, %1037 ], [ %802, %1055 ]
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %9) #10
  %1063 = add nuw nsw i32 %727, 1
  br i1 %903, label %1064, label %725
1064:                                             ; preds = %1060
  store i32 %1061, ptr %249, align 4
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %8) #10
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %7) #10
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %6) #10
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %5) #10
  br label %1065

1065:                                             ; preds = %1064, %211
  call void @llvm.lifetime.start.p0(i64 48, ptr nonnull %14) #10
  %1066 = getelementptr inbounds i8, ptr %13, i64 4
  br label %1067

1067:                                             ; preds = %1105, %1065
  %1068 = phi i32 [ %1112, %1105 ], [ -1, %1065 ]
  %1069 = phi i32 [ %1111, %1105 ], [ -1, %1065 ]
  %1070 = phi i32 [ %1110, %1105 ], [ 0, %1065 ]
  %1071 = phi i64 [ %1106, %1105 ], [ 0, %1065 ]
  %1072 = load i32, ptr %13, align 4, !tbaa !34
  %1073 = icmp eq i32 %1072, 0
  br i1 %1073, label %1079, label %1074

1074:                                             ; preds = %1067
  %1075 = sext i32 %1072 to i64
  br label %1092

1076:                                             ; preds = %1092
  %1077 = trunc i64 %1103 to i8
  %1078 = or i8 %1077, 48
  br label %1079

1079:                                             ; preds = %1076, %1067
  %1080 = phi i8 [ 48, %1067 ], [ %1078, %1076 ]
  br i1 %1073, label %1105, label %1081

1081:                                             ; preds = %1079
  %1082 = sext i32 %1072 to i64
  br label %1083

1083:                                             ; preds = %1089, %1081
  %1084 = phi i64 [ %1082, %1081 ], [ %1085, %1089 ]
  %1085 = add nsw i64 %1084, -1
  %1086 = getelementptr inbounds [1400 x i32], ptr %1066, i64 0, i64 %1085
  %1087 = load i32, ptr %1086, align 4, !tbaa !24
  %1088 = icmp eq i32 %1087, 0
  br i1 %1088, label %1089, label %1105

1089:                                             ; preds = %1083
  %1090 = trunc i64 %1085 to i32
  store i32 %1090, ptr %13, align 4, !tbaa !34
  %1091 = icmp eq i64 %1085, 0
  br i1 %1091, label %1105, label %1083
1092:                                             ; preds = %1092, %1074
  %1093 = phi i64 [ %1075, %1074 ], [ %1095, %1092 ]
  %1094 = phi i64 [ 0, %1074 ], [ %1103, %1092 ]
  %1095 = add nsw i64 %1093, -1
  %1096 = shl nuw nsw i64 %1094, 32
  %1097 = getelementptr inbounds [1400 x i32], ptr %1066, i64 0, i64 %1095
  %1098 = load i32, ptr %1097, align 4, !tbaa !24
  %1099 = zext i32 %1098 to i64
  %1100 = or i64 %1096, %1099
  %1101 = udiv i64 %1100, 10
  %1102 = trunc i64 %1101 to i32
  store i32 %1102, ptr %1097, align 4, !tbaa !24
  %1103 = urem i64 %1100, 10
  %1104 = icmp eq i64 %1095, 0
  br i1 %1104, label %1076, label %1092
1105:                                             ; preds = %1083, %1089, %1079
  %1106 = add nuw nsw i64 %1071, 1
  %1107 = getelementptr inbounds [48 x i8], ptr %14, i64 0, i64 %1071
  store i8 %1080, ptr %1107, align 1, !tbaa !26
  %1108 = load i32, ptr %13, align 4, !tbaa !25
  %1109 = icmp eq i32 %1108, 0
  %1110 = add nuw i32 %1070, 1
  %1111 = add i32 %1069, 1
  %1112 = add i32 %1068, -1
  br i1 %1109, label %1113, label %1067
1113:                                             ; preds = %1105
  %1114 = trunc i64 %1071 to i32
  %1115 = trunc i64 %1106 to i32
  %1116 = icmp eq i64 %1071, 0
  br i1 %1116, label %1137, label %1117

1117:                                             ; preds = %1113
  %1118 = getelementptr inbounds i8, ptr %13, i64 5604
  %1119 = load i32, ptr %1118, align 4
  %1120 = add i32 %1119, %1070
  %1121 = zext i32 %1070 to i64
  br label %1122

1122:                                             ; preds = %1117, %1128
  %1123 = phi i64 [ 0, %1117 ], [ %1130, %1128 ]
  %1124 = phi i32 [ %1119, %1117 ], [ %1129, %1128 ]
  %1125 = getelementptr inbounds [48 x i8], ptr %14, i64 0, i64 %1123
  %1126 = load i8, ptr %1125, align 1, !tbaa !26
  %1127 = icmp eq i8 %1126, 48
  br i1 %1127, label %1128, label %1132

1128:                                             ; preds = %1122
  %1129 = add nsw i32 %1124, 1
  %1130 = add nuw nsw i64 %1123, 1
  %1131 = icmp eq i64 %1130, %1121
  br i1 %1131, label %1134, label %1122
1132:                                             ; preds = %1122
  %1133 = trunc i64 %1123 to i32
  br label %1134

1134:                                             ; preds = %1128, %1132
  %1135 = phi i32 [ %1124, %1132 ], [ %1120, %1128 ]
  %1136 = phi i32 [ %1133, %1132 ], [ %1111, %1128 ]
  store i32 %1135, ptr %1118, align 4
  br label %1137

1137:                                             ; preds = %1134, %1113
  %1138 = phi i32 [ 0, %1113 ], [ %1136, %1134 ]
  %1139 = sub nsw i32 %1115, %1138
  %1140 = getelementptr inbounds i8, ptr %13, i64 5604
  %1141 = load i32, ptr %1140, align 4, !tbaa !35
  %1142 = add nsw i32 %1141, %1139
  %1143 = add nsw i32 %1142, -1
  %1144 = icmp slt i32 %1142, -5
  %1145 = icmp sgt i32 %1141, 6
  %1146 = or i1 %1145, %1144
  br i1 %1146, label %1207, label %1147

1147:                                             ; preds = %1137
  %1148 = icmp slt i32 %1142, 1
  br i1 %1148, label %1149, label %1169

1149:                                             ; preds = %1147
  %1150 = zext i32 %189 to i64
  %1151 = getelementptr inbounds i8, ptr %0, i64 %1150
  store i8 48, ptr %1151, align 1, !tbaa !26
  %1152 = or i32 %189, 2
  %1153 = getelementptr inbounds i8, ptr %1151, i64 1
  store i8 46, ptr %1153, align 1, !tbaa !26
  %1154 = icmp slt i32 %1142, 0
  br i1 %1154, label %1155, label %1169

1155:                                             ; preds = %1149
  %1156 = zext i32 %1152 to i64
  %1157 = sub i32 %1138, %1141
  %1158 = add i32 %1157, %1068
  %1159 = tail call i32 @llvm.smax.i32(i32 %1158, i32 1)
  %1160 = add nuw i32 %1152, %1159
  %1161 = zext i32 %1160 to i64
  br label %1162

1162:                                             ; preds = %1155, %1162
  %1163 = phi i64 [ %1156, %1155 ], [ %1164, %1162 ]
  %1164 = add nuw nsw i64 %1163, 1
  %1165 = getelementptr inbounds i8, ptr %0, i64 %1163
  store i8 48, ptr %1165, align 1, !tbaa !26
  %1166 = icmp eq i64 %1164, %1161
  br i1 %1166, label %1167, label %1162
1167:                                             ; preds = %1162
  %1168 = trunc i64 %1164 to i32
  br label %1169

1169:                                             ; preds = %1167, %1149, %1147
  %1170 = phi i32 [ %189, %1147 ], [ %1152, %1149 ], [ %1168, %1167 ]
  %1171 = icmp sgt i32 %1138, %1114
  br i1 %1171, label %1174, label %1172

1172:                                             ; preds = %1169
  %1173 = sext i32 %1138 to i64
  br label %1180

1174:                                             ; preds = %1197, %1169
  %1175 = phi i32 [ %1170, %1169 ], [ %1198, %1197 ]
  %1176 = phi i32 [ %1142, %1169 ], [ %1141, %1197 ]
  %1177 = icmp sgt i32 %1176, 0
  br i1 %1177, label %1178, label %1275

1178:                                             ; preds = %1174
  %1179 = sext i32 %1175 to i64
  br label %1200

1180:                                             ; preds = %1172, %1197
  %1181 = phi i64 [ %1071, %1172 ], [ %1199, %1197 ]
  %1182 = phi i32 [ %1142, %1172 ], [ %1189, %1197 ]
  %1183 = phi i32 [ %1170, %1172 ], [ %1198, %1197 ]
  %1184 = getelementptr inbounds [48 x i8], ptr %14, i64 0, i64 %1181
  %1185 = load i8, ptr %1184, align 1, !tbaa !26
  %1186 = add nsw i32 %1183, 1
  %1187 = sext i32 %1183 to i64
  %1188 = getelementptr inbounds i8, ptr %0, i64 %1187
  store i8 %1185, ptr %1188, align 1, !tbaa !26
  %1189 = add nsw i32 %1182, -1
  %1190 = icmp eq i32 %1189, 0
  %1191 = icmp sgt i64 %1181, %1173
  %1192 = and i1 %1190, %1191
  br i1 %1192, label %1193, label %1197

1193:                                             ; preds = %1180
  %1194 = add nsw i32 %1183, 2
  %1195 = sext i32 %1186 to i64
  %1196 = getelementptr inbounds i8, ptr %0, i64 %1195
  store i8 46, ptr %1196, align 1, !tbaa !26
  br label %1197

1197:                                             ; preds = %1180, %1193
  %1198 = phi i32 [ %1194, %1193 ], [ %1186, %1180 ]
  %1199 = add nsw i64 %1181, -1
  br i1 %1191, label %1180, label %1174
1200:                                             ; preds = %1178, %1200
  %1201 = phi i64 [ %1179, %1178 ], [ %1204, %1200 ]
  %1202 = phi i32 [ %1176, %1178 ], [ %1203, %1200 ]
  %1203 = add nsw i32 %1202, -1
  %1204 = add nsw i64 %1201, 1
  %1205 = getelementptr inbounds i8, ptr %0, i64 %1201
  store i8 48, ptr %1205, align 1, !tbaa !26
  %1206 = icmp sgt i32 %1202, 1
  br i1 %1206, label %1200, label %1273
1207:                                             ; preds = %1137
  %1208 = add nuw nsw i32 %189, 1
  %1209 = zext i32 %189 to i64
  %1210 = getelementptr inbounds i8, ptr %0, i64 %1209
  store i8 %1080, ptr %1210, align 1, !tbaa !26
  %1211 = icmp sgt i32 %1139, 1
  br i1 %1211, label %1212, label %1216

1212:                                             ; preds = %1207
  %1213 = or i32 %189, 2
  %1214 = zext i32 %1208 to i64
  %1215 = getelementptr inbounds i8, ptr %0, i64 %1214
  store i8 46, ptr %1215, align 1, !tbaa !26
  br label %1216

1216:                                             ; preds = %1212, %1207
  %1217 = phi i32 [ %1213, %1212 ], [ %1208, %1207 ]
  %1218 = icmp slt i32 %1138, %1114
  br i1 %1218, label %1219, label %1226

1219:                                             ; preds = %1216
  %1220 = zext i32 %1217 to i64
  %1221 = sub i32 %1217, %1138
  %1222 = add i32 %1221, %1070
  %1223 = zext i32 %1222 to i64
  br label %1264

1224:                                             ; preds = %1264
  %1225 = trunc i64 %1270 to i32
  br label %1226

1226:                                             ; preds = %1224, %1216
  %1227 = phi i32 [ %1217, %1216 ], [ %1225, %1224 ]
  %1228 = sext i32 %1227 to i64
  %1229 = getelementptr inbounds i8, ptr %0, i64 %1228
  store i8 101, ptr %1229, align 1, !tbaa !26
  %1230 = icmp slt i32 %1142, 1
  %1231 = select i1 %1230, i8 45, i8 43
  %1232 = getelementptr i8, ptr %1229, i64 1
  store i8 %1231, ptr %1232, align 1, !tbaa !26
  %1233 = sub nsw i32 1, %1142
  %1234 = select i1 %1230, i32 %1233, i32 %1143
  call void @llvm.lifetime.start.p0(i64 12, ptr nonnull %4) #10
  br label %1235

1235:                                             ; preds = %1235, %1226
  %1236 = phi i32 [ %1246, %1235 ], [ 1, %1226 ]
  %1237 = phi i64 [ %1242, %1235 ], [ 0, %1226 ]
  %1238 = phi i32 [ %1244, %1235 ], [ %1234, %1226 ]
  %1239 = urem i32 %1238, 10
  %1240 = trunc i32 %1239 to i8
  %1241 = or i8 %1240, 48
  %1242 = add nuw nsw i64 %1237, 1
  %1243 = getelementptr inbounds [12 x i8], ptr %4, i64 0, i64 %1237
  store i8 %1241, ptr %1243, align 1, !tbaa !26
  %1244 = udiv i32 %1238, 10
  %1245 = icmp ult i32 %1238, 10
  %1246 = add nuw i32 %1236, 1
  br i1 %1245, label %1247, label %1235
1247:                                             ; preds = %1235
  %1248 = add nsw i32 %1227, 2
  %1249 = sext i32 %1248 to i64
  %1250 = getelementptr inbounds i8, ptr %0, i64 %1249
  %1251 = and i64 %1237, 4294967295
  %1252 = zext i32 %1236 to i64
  br label %1253

1253:                                             ; preds = %1253, %1247
  %1254 = phi i64 [ 0, %1247 ], [ %1259, %1253 ]
  %1255 = sub nuw nsw i64 %1251, %1254
  %1256 = getelementptr inbounds [12 x i8], ptr %4, i64 0, i64 %1255
  %1257 = load i8, ptr %1256, align 1, !tbaa !26
  %1258 = getelementptr inbounds i8, ptr %1250, i64 %1254
  store i8 %1257, ptr %1258, align 1, !tbaa !26
  %1259 = add nuw nsw i64 %1254, 1
  %1260 = icmp eq i64 %1259, %1252
  br i1 %1260, label %1261, label %1253
1261:                                             ; preds = %1253
  %1262 = trunc i64 %1242 to i32
  call void @llvm.lifetime.end.p0(i64 12, ptr nonnull %4) #10
  %1263 = add nsw i32 %1248, %1262
  br label %1275

1264:                                             ; preds = %1219, %1264
  %1265 = phi i64 [ %1220, %1219 ], [ %1270, %1264 ]
  %1266 = phi i64 [ %1071, %1219 ], [ %1267, %1264 ]
  %1267 = add nsw i64 %1266, -1
  %1268 = getelementptr inbounds [48 x i8], ptr %14, i64 0, i64 %1267
  %1269 = load i8, ptr %1268, align 1, !tbaa !26
  %1270 = add nuw nsw i64 %1265, 1
  %1271 = getelementptr inbounds i8, ptr %0, i64 %1265
  store i8 %1269, ptr %1271, align 1, !tbaa !26
  %1272 = icmp eq i64 %1270, %1223
  br i1 %1272, label %1224, label %1264
1273:                                             ; preds = %1200
  %1274 = trunc i64 %1204 to i32
  br label %1275

1275:                                             ; preds = %1273, %1174, %1261
  %1276 = phi i32 [ %1263, %1261 ], [ %1175, %1174 ], [ %1274, %1273 ]
  call void @llvm.lifetime.end.p0(i64 48, ptr nonnull %14) #10
  br label %1279

1277:                                             ; preds = %195
  %1278 = trunc i64 %200 to i32
  br label %1279

1279:                                             ; preds = %1277, %1275, %207
  %1280 = phi i32 [ %1276, %1275 ], [ %208, %207 ], [ %1278, %1277 ]
  call void @llvm.lifetime.end.p0(i64 5616, ptr nonnull %13) #10
  br label %1281

1281:                                             ; preds = %1279, %177
  %1282 = phi i32 [ %178, %177 ], [ %1280, %1279 ]
  call void @llvm.lifetime.end.p0(i64 36, ptr nonnull %11) #10
  ret i32 %1282
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
  br i1 %11, label %461, label %12

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
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !131
  %14 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 11, ptr %14, align 4, !tbaa !12, !alias.scope !131
  %15 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -24, ptr %15, align 8, !tbaa !13, !alias.scope !131
  %16 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 5, ptr %16, align 4, !tbaa !14, !alias.scope !131
  %17 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 10, ptr %17, align 8, !tbaa !15, !alias.scope !131
  %18 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 15, ptr %18, align 4, !tbaa !16, !alias.scope !131
  %19 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 16, ptr %19, align 8, !tbaa !17, !alias.scope !131
  %20 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %20, align 4, !tbaa !18, !alias.scope !131
  %21 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %21, align 8, !tbaa !19, !alias.scope !131
  br label %89

22:                                               ; preds = %12
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !131
  %23 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 24, ptr %23, align 4, !tbaa !12, !alias.scope !131
  %24 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -149, ptr %24, align 8, !tbaa !13, !alias.scope !131
  %25 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 104, ptr %25, align 4, !tbaa !14, !alias.scope !131
  %26 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 23, ptr %26, align 8, !tbaa !15, !alias.scope !131
  %27 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 127, ptr %27, align 4, !tbaa !16, !alias.scope !131
  %28 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 32, ptr %28, align 8, !tbaa !17, !alias.scope !131
  %29 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %29, align 4, !tbaa !18, !alias.scope !131
  %30 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %30, align 8, !tbaa !19, !alias.scope !131
  br label %89

31:                                               ; preds = %12
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !131
  %32 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 53, ptr %32, align 4, !tbaa !12, !alias.scope !131
  %33 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -1074, ptr %33, align 8, !tbaa !13, !alias.scope !131
  %34 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 971, ptr %34, align 4, !tbaa !14, !alias.scope !131
  %35 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 52, ptr %35, align 8, !tbaa !15, !alias.scope !131
  %36 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 1023, ptr %36, align 4, !tbaa !16, !alias.scope !131
  %37 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 64, ptr %37, align 8, !tbaa !17, !alias.scope !131
  %38 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %38, align 4, !tbaa !18, !alias.scope !131
  %39 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %39, align 8, !tbaa !19, !alias.scope !131
  br label %89

40:                                               ; preds = %12
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !131
  %41 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 113, ptr %41, align 4, !tbaa !12, !alias.scope !131
  %42 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -16494, ptr %42, align 8, !tbaa !13, !alias.scope !131
  %43 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 16271, ptr %43, align 4, !tbaa !14, !alias.scope !131
  %44 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 112, ptr %44, align 8, !tbaa !15, !alias.scope !131
  %45 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 16383, ptr %45, align 4, !tbaa !16, !alias.scope !131
  %46 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 128, ptr %46, align 8, !tbaa !17, !alias.scope !131
  %47 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %47, align 4, !tbaa !18, !alias.scope !131
  %48 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %48, align 8, !tbaa !19, !alias.scope !131
  br label %89

49:                                               ; preds = %12
  store i32 10, ptr %5, align 8, !tbaa !4, !alias.scope !131
  %50 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 7, ptr %50, align 4, !tbaa !12, !alias.scope !131
  %51 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -101, ptr %51, align 8, !tbaa !13, !alias.scope !131
  %52 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 90, ptr %52, align 4, !tbaa !14, !alias.scope !131
  %53 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 23, ptr %53, align 8, !tbaa !15, !alias.scope !131
  %54 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 101, ptr %54, align 4, !tbaa !16, !alias.scope !131
  %55 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 32, ptr %55, align 8, !tbaa !17, !alias.scope !131
  %56 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %56, align 4, !tbaa !18, !alias.scope !131
  %57 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %57, align 8, !tbaa !19, !alias.scope !131
  br label %89

58:                                               ; preds = %12
  store i32 10, ptr %5, align 8, !tbaa !4, !alias.scope !131
  %59 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 16, ptr %59, align 4, !tbaa !12, !alias.scope !131
  %60 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -398, ptr %60, align 8, !tbaa !13, !alias.scope !131
  %61 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 369, ptr %61, align 4, !tbaa !14, !alias.scope !131
  %62 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 53, ptr %62, align 8, !tbaa !15, !alias.scope !131
  %63 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 398, ptr %63, align 4, !tbaa !16, !alias.scope !131
  %64 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 64, ptr %64, align 8, !tbaa !17, !alias.scope !131
  %65 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %65, align 4, !tbaa !18, !alias.scope !131
  %66 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %66, align 8, !tbaa !19, !alias.scope !131
  br label %89

67:                                               ; preds = %12
  store i32 10, ptr %5, align 8, !tbaa !4, !alias.scope !131
  %68 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 34, ptr %68, align 4, !tbaa !12, !alias.scope !131
  %69 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -6176, ptr %69, align 8, !tbaa !13, !alias.scope !131
  %70 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 6111, ptr %70, align 4, !tbaa !14, !alias.scope !131
  %71 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 113, ptr %71, align 8, !tbaa !15, !alias.scope !131
  %72 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 6176, ptr %72, align 4, !tbaa !16, !alias.scope !131
  %73 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 128, ptr %73, align 8, !tbaa !17, !alias.scope !131
  %74 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %74, align 4, !tbaa !18, !alias.scope !131
  %75 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %75, align 8, !tbaa !19, !alias.scope !131
  br label %89

76:                                               ; preds = %12
  %77 = and i32 %3, 7
  %78 = shl nuw nsw i32 8, %77
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !131
  %79 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 %78, ptr %79, align 4, !tbaa !12, !alias.scope !131
  %80 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 0, ptr %80, align 8, !tbaa !13, !alias.scope !131
  %81 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 0, ptr %81, align 4, !tbaa !14, !alias.scope !131
  %82 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 0, ptr %82, align 8, !tbaa !15, !alias.scope !131
  %83 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 0, ptr %83, align 4, !tbaa !16, !alias.scope !131
  %84 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 %78, ptr %84, align 8, !tbaa !17, !alias.scope !131
  %85 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 1, ptr %85, align 4, !tbaa !18, !alias.scope !131
  %86 = getelementptr inbounds i8, ptr %5, i64 32
  %87 = icmp slt i32 %3, 24
  %88 = zext i1 %87 to i32
  store i32 %88, ptr %86, align 8, !tbaa !19, !alias.scope !131
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
  store i64 0, ptr %6, align 8, !tbaa !134
  %100 = load i8, ptr %1, align 1, !tbaa !26
  %101 = icmp eq i8 %100, 45
  %102 = zext i1 %101 to i32
  switch i8 %100, label %104 [
    i8 45, label %103
    i8 43, label %103
  ]

103:                                              ; preds = %89, %89
  store i64 1, ptr %6, align 8, !tbaa !134
  br label %104

104:                                              ; preds = %89, %103
  %105 = load i64, ptr %6, align 8, !tbaa !134
  %106 = icmp eq i64 %105, %2
  br i1 %106, label %459, label %107

107:                                              ; preds = %104
  br i1 %91, label %231, label %108

108:                                              ; preds = %107
  %109 = icmp eq i32 %90, 0
  %110 = and i1 %109, %101
  br i1 %110, label %459, label %111

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
  store i64 %124, ptr %6, align 8, !tbaa !134
  br label %125

125:                                              ; preds = %122, %118, %114, %111
  %126 = phi i32 [ 10, %114 ], [ 10, %111 ], [ 10, %118 ], [ %123, %122 ]
  %127 = load i64, ptr %6, align 8, !tbaa !134
  %128 = icmp eq i64 %127, %2
  br i1 %128, label %459, label %129

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
  %140 = icmp samesign ult i32 %92, 65
  br i1 %140, label %141, label %146

141:                                              ; preds = %129
  %142 = trunc i128 %139 to i64
  %143 = zext i32 %126 to i64
  %144 = udiv i64 %142, %143
  %145 = zext i64 %144 to i128
  br label %149

146:                                              ; preds = %129
  %147 = zext i32 %126 to i128
  %148 = udiv i128 %139, %147
  br label %149

149:                                              ; preds = %146, %141
  %150 = phi i128 [ %145, %141 ], [ %148, %146 ]
  br i1 %140, label %151, label %156

151:                                              ; preds = %149
  %152 = trunc i128 %139 to i64
  %153 = zext i32 %126 to i64
  %154 = urem i64 %152, %153
  %155 = trunc i64 %154 to i32
  br label %160

156:                                              ; preds = %149
  %157 = zext i32 %126 to i128
  %158 = urem i128 %139, %157
  %159 = trunc i128 %158 to i32
  br label %160

160:                                              ; preds = %156, %151
  %161 = phi i32 [ %155, %151 ], [ %159, %156 ]
  %162 = load i64, ptr %6, align 8, !tbaa !134
  %163 = icmp ult i64 %162, %2
  br i1 %163, label %164, label %216

164:                                              ; preds = %160
  %165 = zext i32 %126 to i128
  br label %166

166:                                              ; preds = %164, %211
  %167 = phi i128 [ 0, %164 ], [ %213, %211 ]
  %168 = phi i32 [ 0, %164 ], [ %212, %211 ]
  %169 = phi i64 [ %162, %164 ], [ %214, %211 ]
  %170 = getelementptr inbounds i8, ptr %1, i64 %169
  %171 = load i8, ptr %170, align 1, !tbaa !26
  %172 = icmp eq i8 %171, 95
  br i1 %172, label %173, label %178

173:                                              ; preds = %166
  %174 = icmp eq i32 %168, 0
  %175 = add nuw nsw i64 %169, 1
  %176 = icmp eq i64 %175, %2
  %177 = select i1 %174, i1 true, i1 %176
  br i1 %177, label %458, label %211

178:                                              ; preds = %166
  %179 = sext i8 %171 to i32
  %180 = add i8 %171, -48
  %181 = icmp ult i8 %180, 10
  br i1 %181, label %182, label %184

182:                                              ; preds = %178
  %183 = add nsw i32 %179, -48
  br label %194

184:                                              ; preds = %178
  %185 = add i8 %171, -97
  %186 = icmp ult i8 %185, 6
  br i1 %186, label %187, label %189

187:                                              ; preds = %184
  %188 = add nsw i32 %179, -87
  br label %194

189:                                              ; preds = %184
  %190 = add i8 %171, -65
  %191 = icmp ult i8 %190, 6
  %192 = add nsw i32 %179, -55
  %193 = select i1 %191, i32 %192, i32 -1
  br label %194

194:                                              ; preds = %182, %187, %189
  %195 = phi i32 [ %183, %182 ], [ %188, %187 ], [ %193, %189 ]
  %196 = icmp uge i32 %195, %126
  %197 = icmp ugt i128 %167, %150
  %198 = select i1 %196, i1 true, i1 %197
  br i1 %198, label %207, label %199

199:                                              ; preds = %194
  %200 = icmp eq i128 %167, %150
  %201 = icmp ugt i32 %195, %161
  %202 = select i1 %200, i1 %201, i1 false
  br i1 %202, label %207, label %203

203:                                              ; preds = %199
  %204 = mul i128 %167, %165
  %205 = zext i32 %195 to i128
  %206 = add i128 %204, %205
  br label %207

207:                                              ; preds = %194, %199, %203
  %208 = phi i32 [ 1, %203 ], [ %168, %199 ], [ %168, %194 ]
  %209 = phi i128 [ %206, %203 ], [ %167, %199 ], [ %167, %194 ]
  %210 = phi i1 [ true, %203 ], [ false, %199 ], [ false, %194 ]
  br i1 %210, label %211, label %458

211:                                              ; preds = %173, %207
  %212 = phi i32 [ %208, %207 ], [ 0, %173 ]
  %213 = phi i128 [ %209, %207 ], [ %167, %173 ]
  %214 = add nuw i64 %169, 1
  %215 = icmp eq i64 %214, %2
  br i1 %215, label %216, label %166
216:                                              ; preds = %211, %160
  %217 = phi i64 [ %162, %160 ], [ %2, %211 ]
  %218 = phi i128 [ 0, %160 ], [ %213, %211 ]
  store i64 %217, ptr %6, align 8
  %219 = sub i128 0, %218
  %220 = select i1 %101, i128 %219, i128 %218
  %221 = lshr exact i32 %92, 3
  %222 = zext i32 %221 to i64
  br label %223

223:                                              ; preds = %223, %216
  %224 = phi i64 [ 0, %216 ], [ %229, %223 ]
  %225 = phi i128 [ %220, %216 ], [ %228, %223 ]
  %226 = trunc i128 %225 to i8
  %227 = getelementptr inbounds i8, ptr %0, i64 %224
  store i8 %226, ptr %227, align 1, !tbaa !26
  %228 = lshr i128 %225, 8
  %229 = add nuw nsw i64 %224, 1
  %230 = icmp eq i64 %229, %222
  br i1 %230, label %459, label %223
231:                                              ; preds = %107
  %232 = sub i64 %2, %105
  %233 = icmp eq i64 %232, 3
  br i1 %233, label %234, label %313

234:                                              ; preds = %231
  %235 = getelementptr inbounds i8, ptr %1, i64 %105
  %236 = load i8, ptr %235, align 1, !tbaa !26
  switch i8 %236, label %313 [
    i8 105, label %237
    i8 110, label %269
  ]

237:                                              ; preds = %234
  %238 = getelementptr inbounds i8, ptr %235, i64 1
  %239 = load i8, ptr %238, align 1, !tbaa !26
  %240 = icmp eq i8 %239, 110
  br i1 %240, label %241, label %267

241:                                              ; preds = %237
  %242 = getelementptr inbounds i8, ptr %235, i64 2
  %243 = load i8, ptr %242, align 1, !tbaa !26
  %244 = icmp eq i8 %243, 102
  br i1 %244, label %245, label %267

245:                                              ; preds = %241
  %246 = zext i1 %101 to i128
  %247 = add nsw i32 %92, -1
  %248 = zext i32 %247 to i128
  %249 = shl nuw i128 %246, %248
  %250 = add nsw i32 %92, -6
  %251 = zext i32 %93 to i128
  %252 = select i1 %98, i32 %94, i32 %250
  %253 = select i1 %98, i128 %251, i128 30
  %254 = zext i32 %252 to i128
  %255 = shl i128 %253, %254
  %256 = or i128 %249, %255
  %257 = lshr exact i32 %92, 3
  %258 = zext i32 %257 to i64
  br label %259

259:                                              ; preds = %259, %245
  %260 = phi i64 [ 0, %245 ], [ %265, %259 ]
  %261 = phi i128 [ %256, %245 ], [ %264, %259 ]
  %262 = trunc i128 %261 to i8
  %263 = getelementptr inbounds i8, ptr %0, i64 %260
  store i8 %262, ptr %263, align 1, !tbaa !26
  %264 = lshr i128 %261, 8
  %265 = add nuw nsw i64 %260, 1
  %266 = icmp eq i64 %265, %258
  br i1 %266, label %459, label %259
267:                                              ; preds = %241, %237
  %268 = icmp eq i8 %236, 110
  br i1 %268, label %269, label %313

269:                                              ; preds = %234, %267
  %270 = getelementptr inbounds i8, ptr %235, i64 1
  %271 = load i8, ptr %270, align 1, !tbaa !26
  %272 = icmp eq i8 %271, 97
  br i1 %272, label %273, label %313

273:                                              ; preds = %269
  %274 = getelementptr inbounds i8, ptr %235, i64 2
  %275 = load i8, ptr %274, align 1, !tbaa !26
  %276 = icmp eq i8 %275, 110
  br i1 %276, label %277, label %313

277:                                              ; preds = %273
  br i1 %98, label %278, label %289

278:                                              ; preds = %277
  %279 = zext i32 %93 to i128
  %280 = zext i32 %94 to i128
  %281 = shl nuw i128 %279, %280
  %282 = add nsw i32 %94, -1
  %283 = zext i32 %282 to i128
  %284 = shl nuw i128 1, %283
  %285 = or i128 %284, %281
  %286 = trunc i128 %285 to i64
  %287 = lshr i128 %285, 64
  %288 = trunc i128 %287 to i64
  br label %296

289:                                              ; preds = %277
  %290 = add nsw i32 %92, -6
  %291 = zext i32 %290 to i128
  %292 = shl i128 31, %291
  %293 = trunc i128 %292 to i64
  %294 = lshr i128 %292, 64
  %295 = trunc i128 %294 to i64
  br label %296

296:                                              ; preds = %289, %278
  %297 = phi i64 [ %293, %289 ], [ %286, %278 ]
  %298 = phi i64 [ %295, %289 ], [ %288, %278 ]
  %299 = lshr exact i32 %92, 3
  %300 = zext i64 %298 to i128
  %301 = shl nuw i128 %300, 64
  %302 = zext i64 %297 to i128
  %303 = or i128 %301, %302
  %304 = zext i32 %299 to i64
  br label %305

305:                                              ; preds = %305, %296
  %306 = phi i64 [ 0, %296 ], [ %311, %305 ]
  %307 = phi i128 [ %303, %296 ], [ %310, %305 ]
  %308 = trunc i128 %307 to i8
  %309 = getelementptr inbounds i8, ptr %0, i64 %306
  store i8 %308, ptr %309, align 1, !tbaa !26
  %310 = lshr i128 %307, 8
  %311 = add nuw nsw i64 %306, 1
  %312 = icmp eq i64 %311, %304
  br i1 %312, label %459, label %305
313:                                              ; preds = %234, %273, %269, %267, %231
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %7) #10
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %7, i8 0, i64 5604, i1 false), !alias.scope !137
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %8) #10
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %8, i8 0, i64 5604, i1 false), !alias.scope !140
  %314 = getelementptr inbounds i8, ptr %8, i64 4
  store i32 1, ptr %8, align 4, !tbaa !34, !alias.scope !140
  store i32 1, ptr %314, align 4, !tbaa !24, !alias.scope !140
  %315 = call fastcc i32 @decimal_digits(ptr noundef nonnull %1, i64 noundef %2, ptr noundef %6, ptr noundef nonnull %7, ptr noundef null) #11
  %316 = icmp slt i32 %315, 0
  br i1 %316, label %456, label %317

317:                                              ; preds = %313
  %318 = load i64, ptr %6, align 8, !tbaa !134
  %319 = icmp ult i64 %318, %2
  br i1 %319, label %320, label %328

320:                                              ; preds = %317
  %321 = getelementptr inbounds i8, ptr %1, i64 %318
  %322 = load i8, ptr %321, align 1, !tbaa !26
  %323 = icmp eq i8 %322, 46
  br i1 %323, label %324, label %328

324:                                              ; preds = %320
  %325 = add nuw nsw i64 %318, 1
  store i64 %325, ptr %6, align 8, !tbaa !134
  %326 = call fastcc i32 @decimal_digits(ptr noundef nonnull %1, i64 noundef %2, ptr noundef %6, ptr noundef nonnull %7, ptr noundef null) #11
  %327 = icmp slt i32 %326, 0
  br i1 %327, label %456, label %328

328:                                              ; preds = %324, %320, %317
  %329 = phi i32 [ %326, %324 ], [ 0, %320 ], [ 0, %317 ]
  %330 = or i32 %329, %315
  %331 = icmp eq i32 %330, 0
  br i1 %331, label %456, label %332

332:                                              ; preds = %328
  call void @llvm.lifetime.start.p0(i64 4, ptr nonnull %9) #10
  store i32 0, ptr %9, align 4, !tbaa !24
  %333 = load i64, ptr %6, align 8, !tbaa !134
  %334 = icmp ult i64 %333, %2
  br i1 %334, label %335, label %364

335:                                              ; preds = %332
  %336 = getelementptr inbounds i8, ptr %1, i64 %333
  %337 = load i8, ptr %336, align 1, !tbaa !26
  switch i8 %337, label %364 [
    i8 101, label %338
    i8 69, label %338
  ]

338:                                              ; preds = %335, %335
  %339 = add i64 %333, 1
  store i64 %339, ptr %6, align 8, !tbaa !134
  %340 = icmp ult i64 %339, %2
  br i1 %340, label %341, label %345

341:                                              ; preds = %338
  %342 = getelementptr inbounds i8, ptr %1, i64 %339
  %343 = load i8, ptr %342, align 1, !tbaa !26
  %344 = icmp eq i8 %343, 45
  br label %345

345:                                              ; preds = %341, %338
  %346 = phi i1 [ false, %338 ], [ %344, %341 ]
  br i1 %340, label %347, label %354

347:                                              ; preds = %345
  br i1 %346, label %352, label %348

348:                                              ; preds = %347
  %349 = getelementptr inbounds i8, ptr %1, i64 %339
  %350 = load i8, ptr %349, align 1, !tbaa !26
  %351 = icmp eq i8 %350, 43
  br i1 %351, label %352, label %354

352:                                              ; preds = %348, %347
  %353 = add i64 %333, 2
  store i64 %353, ptr %6, align 8, !tbaa !134
  br label %354

354:                                              ; preds = %352, %348, %345
  %355 = call fastcc i32 @decimal_digits(ptr noundef nonnull %1, i64 noundef %2, ptr noundef %6, ptr noundef null, ptr noundef nonnull %9) #11
  %356 = icmp sgt i32 %355, 0
  %357 = select i1 %356, i1 %346, i1 false
  %358 = zext i1 %356 to i32
  br i1 %357, label %359, label %362

359:                                              ; preds = %354
  %360 = load i32, ptr %9, align 4, !tbaa !24
  %361 = sub nsw i32 0, %360
  store i32 %361, ptr %9, align 4, !tbaa !24
  br label %362

362:                                              ; preds = %354, %359
  %363 = phi i32 [ %358, %354 ], [ 1, %359 ]
  br i1 %356, label %364, label %454

364:                                              ; preds = %335, %362, %332
  %365 = load i64, ptr %6, align 8, !tbaa !134
  %366 = icmp eq i64 %365, %2
  br i1 %366, label %367, label %454

367:                                              ; preds = %364
  %368 = load i32, ptr %9, align 4, !tbaa !24
  %369 = sub nsw i32 %368, %329
  store i32 %369, ptr %9, align 4, !tbaa !24
  %370 = load i32, ptr %7, align 4, !tbaa !34
  %371 = icmp eq i32 %370, 0
  br i1 %371, label %372, label %390

372:                                              ; preds = %367
  %373 = call fastcc { i64, i64 } @pack(ptr noundef %7, ptr noundef %8, i32 noundef %369, i32 noundef %102, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %5) #11
  %374 = extractvalue { i64, i64 } %373, 1
  %375 = extractvalue { i64, i64 } %373, 0
  %376 = lshr exact i32 %92, 3
  %377 = zext i64 %374 to i128
  %378 = shl nuw i128 %377, 64
  %379 = zext i64 %375 to i128
  %380 = or i128 %378, %379
  %381 = zext i32 %376 to i64
  br label %382

382:                                              ; preds = %382, %372
  %383 = phi i64 [ 0, %372 ], [ %388, %382 ]
  %384 = phi i128 [ %380, %372 ], [ %387, %382 ]
  %385 = trunc i128 %384 to i8
  %386 = getelementptr inbounds i8, ptr %0, i64 %383
  store i8 %385, ptr %386, align 1, !tbaa !26
  %387 = lshr i128 %384, 8
  %388 = add nuw nsw i64 %383, 1
  %389 = icmp eq i64 %388, %381
  br i1 %389, label %454, label %382
390:                                              ; preds = %367
  %391 = call fastcc i32 @magnitude(ptr noundef %7, ptr noundef %8, i32 noundef 10) #11
  %392 = add nsw i32 %391, %369
  %393 = add nuw nsw i32 %97, %95
  br i1 %99, label %394, label %396

394:                                              ; preds = %390
  %395 = add nsw i32 %393, -1
  br label %400

396:                                              ; preds = %390
  %397 = mul nuw nsw i32 %393, 30103
  %398 = udiv i32 %397, 100000
  %399 = add nuw nsw i32 %398, 1
  br label %400

400:                                              ; preds = %396, %394
  %401 = phi i32 [ %395, %394 ], [ %399, %396 ]
  br i1 %99, label %402, label %404

402:                                              ; preds = %400
  %403 = add nsw i32 %96, -1
  br label %408

404:                                              ; preds = %400
  %405 = mul nsw i32 %96, 30103
  %406 = sdiv i32 %405, 100000
  %407 = add nsw i32 %406, -2
  br label %408

408:                                              ; preds = %404, %402
  %409 = phi i32 [ %403, %402 ], [ %407, %404 ]
  %410 = icmp sgt i32 %392, %401
  br i1 %410, label %454, label %411

411:                                              ; preds = %408
  %412 = icmp slt i32 %392, %409
  br i1 %412, label %413, label %414

413:                                              ; preds = %411
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %7, i8 0, i64 5604, i1 false)
  br label %420

414:                                              ; preds = %411
  br i1 %98, label %415, label %422

415:                                              ; preds = %414
  %416 = icmp sgt i32 %369, -1
  br i1 %416, label %417, label %418

417:                                              ; preds = %415
  call fastcc void @power(ptr noundef %7, i32 noundef 10, i32 noundef %369) #11
  br label %420

418:                                              ; preds = %415
  %419 = sub nsw i32 0, %369
  call fastcc void @power(ptr noundef %8, i32 noundef 10, i32 noundef %419) #11
  br label %420

420:                                              ; preds = %417, %418, %413
  %421 = phi i32 [ %96, %413 ], [ 0, %418 ], [ 0, %417 ]
  store i32 %421, ptr %9, align 4, !tbaa !24
  br label %422

422:                                              ; preds = %420, %414
  %423 = load i32, ptr %9, align 4, !tbaa !24
  %424 = call fastcc { i64, i64 } @pack(ptr noundef %7, ptr noundef %8, i32 noundef %423, i32 noundef %102, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %5) #11
  %425 = extractvalue { i64, i64 } %424, 0
  %426 = extractvalue { i64, i64 } %424, 1
  %427 = zext i64 %426 to i128
  %428 = shl nuw i128 %427, 64
  %429 = zext i64 %425 to i128
  %430 = or i128 %428, %429
  %431 = add nsw i32 %92, -1
  %432 = zext i32 %431 to i128
  %433 = shl nsw i128 -1, %432
  %434 = xor i128 %433, -1
  %435 = and i128 %430, %434
  %436 = zext i32 %93 to i128
  %437 = add nsw i32 %92, -6
  %438 = select i1 %98, i32 %94, i32 %437
  %439 = select i1 %98, i128 %436, i128 30
  %440 = zext i32 %438 to i128
  %441 = shl i128 %439, %440
  %442 = icmp eq i128 %435, %441
  br i1 %442, label %454, label %443

443:                                              ; preds = %422
  %444 = lshr exact i32 %92, 3
  %445 = zext i32 %444 to i64
  br label %446

446:                                              ; preds = %446, %443
  %447 = phi i64 [ 0, %443 ], [ %452, %446 ]
  %448 = phi i128 [ %430, %443 ], [ %451, %446 ]
  %449 = trunc i128 %448 to i8
  %450 = getelementptr inbounds i8, ptr %0, i64 %447
  store i8 %449, ptr %450, align 1, !tbaa !26
  %451 = lshr i128 %448, 8
  %452 = add nuw nsw i64 %447, 1
  %453 = icmp eq i64 %452, %445
  br i1 %453, label %454, label %446
454:                                              ; preds = %446, %382, %408, %422, %364, %362
  %455 = phi i32 [ %363, %362 ], [ 0, %364 ], [ 0, %408 ], [ 0, %422 ], [ 1, %382 ], [ 1, %446 ]
  call void @llvm.lifetime.end.p0(i64 4, ptr nonnull %9) #10
  br label %456

456:                                              ; preds = %454, %324, %328, %313
  %457 = phi i32 [ 0, %313 ], [ %455, %454 ], [ 0, %324 ], [ 0, %328 ]
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %8) #10
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %7) #10
  br label %459

458:                                              ; preds = %207, %173
  store i64 %169, ptr %6, align 8
  br label %459

459:                                              ; preds = %223, %305, %259, %458, %125, %108, %104, %456
  %460 = phi i32 [ %457, %456 ], [ 0, %104 ], [ 0, %108 ], [ 0, %125 ], [ 0, %458 ], [ 1, %259 ], [ 1, %305 ], [ 1, %223 ]
  call void @llvm.lifetime.end.p0(i64 8, ptr nonnull %6) #10
  call void @llvm.lifetime.end.p0(i64 36, ptr nonnull %5) #10
  br label %461

461:                                              ; preds = %4, %459
  %462 = phi i32 [ %460, %459 ], [ 0, %4 ]
  ret i32 %462
}

; Function Attrs: nounwind memory(argmem: readwrite, inaccessiblemem: write)
define internal fastcc i32 @decimal_digits(ptr noundef readonly %0, i64 noundef %1, ptr noundef nonnull %2, ptr noundef %3, ptr noundef %4) unnamed_addr #2 {
  %6 = load i64, ptr %2, align 8, !tbaa !134
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
  store i64 %94, ptr %2, align 8, !tbaa !134
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
!105 = distinct !{!105, !28, !29}
!106 = !{!107}
!107 = distinct !{!107, !108, !"small: argument 0"}
!108 = distinct !{!108, !"small"}
!109 = !{!110}
!110 = distinct !{!110, !111, !"small: argument 0"}
!111 = distinct !{!111, !"small"}
!112 = !{!113}
!113 = distinct !{!113, !114, !"small: argument 0"}
!114 = distinct !{!114, !"small"}
!115 = !{!116}
!116 = distinct !{!116, !117, !"small: argument 0"}
!117 = distinct !{!117, !"small"}
!118 = distinct !{!118, !28, !29}
!119 = !{!120}
!120 = distinct !{!120, !121, !"small: argument 0"}
!121 = distinct !{!121, !"small"}
!122 = distinct !{!122, !29}
!123 = distinct !{!123, !28, !29}
!124 = distinct !{!124, !28, !29}
!125 = distinct !{!125, !28, !29}
!126 = distinct !{!126, !28, !29}
!127 = distinct !{!127, !28, !29}
!128 = distinct !{!128, !28, !29}
!129 = distinct !{!129, !28, !29}
!130 = distinct !{!130, !28, !29}
!131 = !{!132}
!132 = distinct !{!132, !133, !"format: argument 0"}
!133 = distinct !{!133, !"format"}
!134 = !{!135, !135, i64 0}
!135 = !{!"long long", !7, i64 0}
!136 = distinct !{!136, !28, !29}
!137 = !{!138}
!138 = distinct !{!138, !139, !"small: argument 0"}
!139 = distinct !{!139, !"small"}
!140 = !{!141}
!141 = distinct !{!141, !142, !"small: argument 0"}
!142 = distinct !{!142, !"small"}
!143 = distinct !{!143, !28, !29}
!144 = distinct !{!144, !28, !29}
!145 = distinct !{!145, !28, !29}
!146 = distinct !{!146, !28, !29}
