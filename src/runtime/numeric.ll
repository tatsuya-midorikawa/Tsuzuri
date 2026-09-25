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
  call void @llvm.lifetime.start.p0(i64 36, ptr nonnull %6) #9
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
  call void @llvm.lifetime.start.p0(i64 5616, ptr nonnull %7) #9
  call fastcc void @decode(ptr sret(%struct.tzrt_number) align 4 %7, ptr noundef %1, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %6) #10
  call void @llvm.lifetime.start.p0(i64 5616, ptr nonnull %8) #9
  call fastcc void @decode(ptr sret(%struct.tzrt_number) align 4 %8, ptr noundef %2, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %6) #10
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
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %9) #9
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %9, ptr noundef nonnull align 4 dereferenceable(5604) %7, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %10) #9
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
  call fastcc void @power(ptr noundef %9, i32 noundef %93, i32 noundef %220) #10
  %221 = sub nsw i32 %218, %219
  call fastcc void @power(ptr noundef %8, i32 noundef %93, i32 noundef %221) #10
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
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %11) #9
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
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %11) #9
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
  %462 = call fastcc { i64, i64 } @pack(ptr noundef %9, ptr noundef %10, i32 noundef %460, i32 noundef %461, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %6) #10
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
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %10) #9
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %9) #9
  br label %480

480:                                              ; preds = %204, %479
  call void @llvm.lifetime.end.p0(i64 5616, ptr nonnull %8) #9
  call void @llvm.lifetime.end.p0(i64 5616, ptr nonnull %7) #9
  call void @llvm.lifetime.end.p0(i64 36, ptr nonnull %6) #9
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
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %4) #9
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
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %4) #9
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
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %5) #9
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
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %5) #9
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
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %6) #9
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
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %6) #9
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %7) #9
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
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %7) #9
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

; Function Attrs: nounwind memory(argmem: readwrite, inaccessiblemem: write)
define internal fastcc { i64, i64 } @pack(ptr noundef nonnull readonly %0, ptr noundef nonnull readonly %1, i32 noundef %2, i32 noundef %3, ptr noundef readonly byval(%struct.tzrt_format) align 8 %4) unnamed_addr #2 {
  %6 = alloca %struct.tzrt_big, align 4
  %7 = alloca %struct.tzrt_big, align 4
  %8 = alloca %struct.tzrt_big, align 4
  %9 = sext i32 %3 to i128
  %10 = getelementptr inbounds i8, ptr %4, i64 24
  %11 = load i32, ptr %10, align 8, !tbaa !17
  %12 = add nsw i32 %11, -1
  %13 = zext i32 %12 to i128
  %14 = shl i128 %9, %13
  %15 = load i32, ptr %0, align 4, !tbaa !34
  %16 = icmp eq i32 %15, 0
  %17 = load i32, ptr %4, align 8, !tbaa !4
  br i1 %16, label %18, label %42

18:                                               ; preds = %5
  %19 = icmp eq i32 %17, 10
  br i1 %19, label %20, label %36

20:                                               ; preds = %18
  %21 = getelementptr inbounds i8, ptr %4, i64 8
  %22 = load i32, ptr %21, align 8, !tbaa !13
  %23 = icmp slt i32 %2, %22
  %24 = getelementptr inbounds i8, ptr %4, i64 12
  %25 = load i32, ptr %24, align 4
  %26 = tail call i32 @llvm.smin.i32(i32 %2, i32 %25)
  %27 = select i1 %23, i32 %22, i32 %26
  %28 = getelementptr inbounds i8, ptr %4, i64 20
  %29 = load i32, ptr %28, align 4, !tbaa !16
  %30 = add nsw i32 %27, %29
  %31 = sext i32 %30 to i128
  %32 = getelementptr inbounds i8, ptr %4, i64 16
  %33 = load i32, ptr %32, align 8, !tbaa !15
  %34 = zext i32 %33 to i128
  %35 = shl i128 %31, %34
  br label %36

36:                                               ; preds = %18, %20
  %37 = phi i128 [ %35, %20 ], [ 0, %18 ]
  %38 = or i128 %37, %14
  %39 = trunc i128 %38 to i64
  %40 = lshr i128 %38, 64
  %41 = trunc i128 %40 to i64
  br label %318

42:                                               ; preds = %5
  %43 = tail call fastcc i32 @magnitude(ptr noundef %0, ptr noundef %1, i32 noundef %17) #10
  %44 = getelementptr inbounds i8, ptr %4, i64 4
  %45 = load i32, ptr %44, align 4, !tbaa !12
  %46 = add i32 %2, 1
  %47 = add i32 %46, %43
  %48 = sub i32 %47, %45
  %49 = getelementptr inbounds i8, ptr %4, i64 8
  %50 = load i32, ptr %49, align 8, !tbaa !13
  %51 = tail call i32 @llvm.smax.i32(i32 %48, i32 %50)
  %52 = getelementptr inbounds i8, ptr %4, i64 12
  %53 = load i32, ptr %52, align 4, !tbaa !14
  %54 = icmp sgt i32 %51, %53
  br i1 %54, label %55, label %79

55:                                               ; preds = %42
  %56 = icmp eq i32 %17, 2
  br i1 %56, label %57, label %71

57:                                               ; preds = %55
  %58 = getelementptr inbounds i8, ptr %4, i64 20
  %59 = load i32, ptr %58, align 4, !tbaa !16
  %60 = shl nsw i32 %59, 1
  %61 = or i32 %60, 1
  %62 = sext i32 %61 to i128
  %63 = getelementptr inbounds i8, ptr %4, i64 16
  %64 = load i32, ptr %63, align 8, !tbaa !15
  %65 = zext i32 %64 to i128
  %66 = shl i128 %62, %65
  %67 = or i128 %66, %14
  %68 = trunc i128 %67 to i64
  %69 = lshr i128 %67, 64
  %70 = trunc i128 %69 to i64
  br label %318

71:                                               ; preds = %55
  %72 = add nsw i32 %11, -6
  %73 = zext i32 %72 to i128
  %74 = shl i128 30, %73
  %75 = or i128 %14, %74
  %76 = trunc i128 %75 to i64
  %77 = lshr i128 %75, 64
  %78 = trunc i128 %77 to i64
  br label %318

79:                                               ; preds = %42
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %6) #9
  %80 = sub nsw i32 %2, %51
  call fastcc void @rounded(ptr sret(%struct.tzrt_big) align 4 %6, ptr noundef %0, ptr noundef %1, i32 noundef %17, i32 noundef %80) #10
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %7) #9
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %7, i8 0, i64 5604, i1 false), !alias.scope !65
  %81 = getelementptr inbounds i8, ptr %7, i64 4
  store i32 1, ptr %7, align 4, !tbaa !34, !alias.scope !65
  store i32 1, ptr %81, align 4, !tbaa !24, !alias.scope !65
  call fastcc void @power(ptr noundef %7, i32 noundef %17, i32 noundef %45) #10
  %82 = load i32, ptr %6, align 4, !tbaa !34
  %83 = load i32, ptr %7, align 4, !tbaa !34
  %84 = icmp eq i32 %82, %83
  br i1 %84, label %85, label %90

85:                                               ; preds = %79
  %86 = getelementptr inbounds i8, ptr %6, i64 4
  %87 = icmp eq i32 %82, 0
  br i1 %87, label %106, label %88

88:                                               ; preds = %85
  %89 = sext i32 %82 to i64
  br label %95

90:                                               ; preds = %79
  %91 = icmp slt i32 %82, %83
  %92 = select i1 %91, i32 -1, i32 1
  br label %106

93:                                               ; preds = %95
  %94 = icmp eq i64 %97, 0
  br i1 %94, label %106, label %95
95:                                               ; preds = %88, %93
  %96 = phi i64 [ %89, %88 ], [ %97, %93 ]
  %97 = add nsw i64 %96, -1
  %98 = getelementptr inbounds [1400 x i32], ptr %86, i64 0, i64 %97
  %99 = load i32, ptr %98, align 4, !tbaa !24
  %100 = getelementptr inbounds [1400 x i32], ptr %81, i64 0, i64 %97
  %101 = load i32, ptr %100, align 4, !tbaa !24
  %102 = icmp eq i32 %99, %101
  br i1 %102, label %93, label %103
103:                                              ; preds = %95
  %104 = icmp ult i32 %99, %101
  %105 = select i1 %104, i32 -1, i32 1
  br label %106

106:                                              ; preds = %93, %85, %90, %103
  %107 = phi i32 [ %92, %90 ], [ %105, %103 ], [ 0, %85 ], [ 0, %93 ]
  %108 = icmp sgt i32 %107, -1
  br i1 %108, label %109, label %168

109:                                              ; preds = %106
  %110 = icmp eq i32 %82, 0
  br i1 %110, label %115, label %111

111:                                              ; preds = %109
  %112 = getelementptr inbounds i8, ptr %6, i64 4
  %113 = zext i32 %17 to i64
  %114 = sext i32 %82 to i64
  br label %128

115:                                              ; preds = %128, %109
  %116 = getelementptr inbounds i8, ptr %6, i64 4
  br i1 %110, label %141, label %117

117:                                              ; preds = %115
  %118 = sext i32 %82 to i64
  br label %119

119:                                              ; preds = %125, %117
  %120 = phi i64 [ %118, %117 ], [ %121, %125 ]
  %121 = add nsw i64 %120, -1
  %122 = getelementptr inbounds [1400 x i32], ptr %116, i64 0, i64 %121
  %123 = load i32, ptr %122, align 4, !tbaa !24
  %124 = icmp eq i32 %123, 0
  br i1 %124, label %125, label %141

125:                                              ; preds = %119
  %126 = trunc i64 %121 to i32
  store i32 %126, ptr %6, align 4, !tbaa !34
  %127 = icmp eq i64 %121, 0
  br i1 %127, label %141, label %119
128:                                              ; preds = %128, %111
  %129 = phi i64 [ %114, %111 ], [ %131, %128 ]
  %130 = phi i64 [ 0, %111 ], [ %139, %128 ]
  %131 = add nsw i64 %129, -1
  %132 = shl nuw i64 %130, 32
  %133 = getelementptr inbounds [1400 x i32], ptr %112, i64 0, i64 %131
  %134 = load i32, ptr %133, align 4, !tbaa !24
  %135 = zext i32 %134 to i64
  %136 = or i64 %132, %135
  %137 = udiv i64 %136, %113
  %138 = trunc i64 %137 to i32
  store i32 %138, ptr %133, align 4, !tbaa !24
  %139 = urem i64 %136, %113
  %140 = icmp eq i64 %131, 0
  br i1 %140, label %115, label %128
141:                                              ; preds = %119, %125, %115
  %142 = add nsw i32 %51, 1
  %143 = icmp slt i32 %51, %53
  br i1 %143, label %168, label %144

144:                                              ; preds = %141
  %145 = icmp eq i32 %17, 2
  br i1 %145, label %146, label %160

146:                                              ; preds = %144
  %147 = getelementptr inbounds i8, ptr %4, i64 20
  %148 = load i32, ptr %147, align 4, !tbaa !16
  %149 = shl nsw i32 %148, 1
  %150 = or i32 %149, 1
  %151 = sext i32 %150 to i128
  %152 = getelementptr inbounds i8, ptr %4, i64 16
  %153 = load i32, ptr %152, align 8, !tbaa !15
  %154 = zext i32 %153 to i128
  %155 = shl i128 %151, %154
  %156 = or i128 %155, %14
  %157 = trunc i128 %156 to i64
  %158 = lshr i128 %156, 64
  %159 = trunc i128 %158 to i64
  br label %315

160:                                              ; preds = %144
  %161 = add nsw i32 %11, -6
  %162 = zext i32 %161 to i128
  %163 = shl i128 30, %162
  %164 = or i128 %14, %163
  %165 = trunc i128 %164 to i64
  %166 = lshr i128 %164, 64
  %167 = trunc i128 %166 to i64
  br label %315

168:                                              ; preds = %141, %106
  %169 = phi i32 [ %142, %141 ], [ %51, %106 ]
  %170 = icmp eq i32 %17, 10
  br i1 %170, label %171, label %219

171:                                              ; preds = %168
  %172 = getelementptr inbounds i8, ptr %8, i64 4
  br label %173

173:                                              ; preds = %171, %217
  %174 = phi i32 [ %218, %217 ], [ %169, %171 ]
  %175 = icmp slt i32 %174, %2
  br i1 %175, label %176, label %219

176:                                              ; preds = %173
  %177 = icmp slt i32 %174, %53
  %178 = load i32, ptr %6, align 4
  %179 = icmp ne i32 %178, 0
  %180 = select i1 %177, i1 %179, i1 false
  br i1 %180, label %181, label %219

181:                                              ; preds = %176
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %8) #9
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %8, ptr noundef nonnull align 4 dereferenceable(5604) %6, i64 5604, i1 false), !tbaa.struct !30
  %182 = load i32, ptr %8, align 4, !tbaa !34
  %183 = icmp eq i32 %182, 0
  br i1 %183, label %188, label %184

184:                                              ; preds = %181
  %185 = sext i32 %182 to i64
  br label %201

186:                                              ; preds = %201
  %187 = icmp eq i64 %212, 0
  br label %188

188:                                              ; preds = %186, %181
  %189 = phi i1 [ true, %181 ], [ %187, %186 ]
  br i1 %183, label %214, label %190

190:                                              ; preds = %188
  %191 = sext i32 %182 to i64
  br label %192

192:                                              ; preds = %198, %190
  %193 = phi i64 [ %191, %190 ], [ %194, %198 ]
  %194 = add nsw i64 %193, -1
  %195 = getelementptr inbounds [1400 x i32], ptr %172, i64 0, i64 %194
  %196 = load i32, ptr %195, align 4, !tbaa !24
  %197 = icmp eq i32 %196, 0
  br i1 %197, label %198, label %214

198:                                              ; preds = %192
  %199 = trunc i64 %194 to i32
  store i32 %199, ptr %8, align 4, !tbaa !34
  %200 = icmp eq i64 %194, 0
  br i1 %200, label %214, label %192
201:                                              ; preds = %201, %184
  %202 = phi i64 [ %185, %184 ], [ %204, %201 ]
  %203 = phi i64 [ 0, %184 ], [ %212, %201 ]
  %204 = add nsw i64 %202, -1
  %205 = shl nuw nsw i64 %203, 32
  %206 = getelementptr inbounds [1400 x i32], ptr %172, i64 0, i64 %204
  %207 = load i32, ptr %206, align 4, !tbaa !24
  %208 = zext i32 %207 to i64
  %209 = or i64 %205, %208
  %210 = udiv i64 %209, 10
  %211 = trunc i64 %210 to i32
  store i32 %211, ptr %206, align 4, !tbaa !24
  %212 = urem i64 %209, 10
  %213 = icmp eq i64 %204, 0
  br i1 %213, label %186, label %201
214:                                              ; preds = %192, %198, %188
  br i1 %189, label %215, label %217

215:                                              ; preds = %214
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %6, ptr noundef nonnull align 4 dereferenceable(5604) %8, i64 5604, i1 false), !tbaa.struct !30
  %216 = add nsw i32 %174, 1
  br label %217

217:                                              ; preds = %214, %215
  %218 = phi i32 [ %216, %215 ], [ %174, %214 ]
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %8) #9
  br i1 %189, label %173, label %219

219:                                              ; preds = %217, %173, %176, %168
  %220 = phi i32 [ %169, %168 ], [ %218, %217 ], [ %174, %176 ], [ %174, %173 ]
  %221 = load i32, ptr %6, align 4, !tbaa !34
  %222 = icmp sgt i32 %221, 4
  br i1 %222, label %228, label %223

223:                                              ; preds = %219
  %224 = icmp eq i32 %221, 0
  br i1 %224, label %243, label %225

225:                                              ; preds = %223
  %226 = getelementptr inbounds i8, ptr %6, i64 4
  %227 = sext i32 %221 to i64
  br label %233

228:                                              ; preds = %219
  tail call void @llvm.trap()
  unreachable

229:                                              ; preds = %233
  %230 = lshr i128 %237, 64
  %231 = trunc i128 %230 to i64
  %232 = trunc i128 %241 to i64
  br label %243

233:                                              ; preds = %233, %225
  %234 = phi i64 [ %227, %225 ], [ %236, %233 ]
  %235 = phi i128 [ 0, %225 ], [ %241, %233 ]
  %236 = add nsw i64 %234, -1
  %237 = shl i128 %235, 32
  %238 = getelementptr inbounds [1400 x i32], ptr %226, i64 0, i64 %236
  %239 = load i32, ptr %238, align 4, !tbaa !24
  %240 = zext i32 %239 to i128
  %241 = or i128 %237, %240
  %242 = icmp eq i64 %236, 0
  br i1 %242, label %229, label %233
243:                                              ; preds = %223, %229
  %244 = phi i64 [ 0, %223 ], [ %232, %229 ]
  %245 = phi i64 [ 0, %223 ], [ %231, %229 ]
  %246 = zext i64 %245 to i128
  %247 = shl nuw i128 %246, 64
  %248 = zext i64 %244 to i128
  %249 = or i128 %247, %248
  %250 = icmp eq i32 %17, 2
  br i1 %250, label %251, label %274

251:                                              ; preds = %243
  %252 = getelementptr inbounds i8, ptr %4, i64 16
  %253 = load i32, ptr %252, align 8, !tbaa !15
  %254 = zext i32 %253 to i128
  %255 = lshr i128 %249, %254
  %256 = icmp eq i128 %255, 0
  %257 = add nsw i32 %253, %220
  %258 = getelementptr inbounds i8, ptr %4, i64 20
  %259 = load i32, ptr %258, align 4
  %260 = add nsw i32 %257, %259
  %261 = sext i32 %260 to i128
  %262 = select i1 %256, i128 0, i128 %261
  %263 = shl i128 %262, %254
  %264 = icmp eq i32 %253, 128
  %265 = shl nsw i128 -1, %254
  %266 = xor i128 %265, -1
  %267 = select i1 %264, i128 -1, i128 %266
  %268 = and i128 %267, %249
  %269 = or i128 %268, %263
  %270 = or i128 %269, %14
  %271 = trunc i128 %270 to i64
  %272 = lshr i128 %270, 64
  %273 = trunc i128 %272 to i64
  br label %315

274:                                              ; preds = %243
  %275 = getelementptr inbounds i8, ptr %4, i64 20
  %276 = load i32, ptr %275, align 4, !tbaa !16
  %277 = add nsw i32 %276, %220
  %278 = icmp eq i32 %11, 128
  br i1 %278, label %304, label %279

279:                                              ; preds = %274
  %280 = getelementptr inbounds i8, ptr %4, i64 16
  %281 = load i32, ptr %280, align 8, !tbaa !15
  %282 = zext i32 %281 to i128
  %283 = lshr i128 %249, %282
  %284 = icmp eq i128 %283, 0
  br i1 %284, label %304, label %285

285:                                              ; preds = %279
  %286 = add nsw i32 %11, -3
  %287 = zext i32 %286 to i128
  %288 = shl i128 3, %287
  %289 = sext i32 %277 to i128
  %290 = add nsw i32 %281, -2
  %291 = zext i32 %290 to i128
  %292 = shl i128 %289, %291
  %293 = icmp eq i32 %290, 128
  %294 = shl nsw i128 -1, %291
  %295 = xor i128 %294, -1
  %296 = select i1 %293, i128 -1, i128 %295
  %297 = and i128 %296, %249
  %298 = or i128 %288, %292
  %299 = or i128 %298, %297
  %300 = or i128 %299, %14
  %301 = trunc i128 %300 to i64
  %302 = lshr i128 %300, 64
  %303 = trunc i128 %302 to i64
  br label %315

304:                                              ; preds = %279, %274
  %305 = sext i32 %277 to i128
  %306 = getelementptr inbounds i8, ptr %4, i64 16
  %307 = load i32, ptr %306, align 8, !tbaa !15
  %308 = zext i32 %307 to i128
  %309 = shl i128 %305, %308
  %310 = or i128 %14, %309
  %311 = or i128 %310, %249
  %312 = trunc i128 %311 to i64
  %313 = lshr i128 %311, 64
  %314 = trunc i128 %313 to i64
  br label %315

315:                                              ; preds = %160, %146, %251, %304, %285
  %316 = phi i64 [ %165, %160 ], [ %157, %146 ], [ %271, %251 ], [ %312, %304 ], [ %301, %285 ]
  %317 = phi i64 [ %167, %160 ], [ %159, %146 ], [ %273, %251 ], [ %314, %304 ], [ %303, %285 ]
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %7) #9
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %6) #9
  br label %318

318:                                              ; preds = %71, %57, %315, %36
  %319 = phi i64 [ %39, %36 ], [ %316, %315 ], [ %68, %57 ], [ %76, %71 ]
  %320 = phi i64 [ %41, %36 ], [ %317, %315 ], [ %70, %57 ], [ %78, %71 ]
  %321 = insertvalue { i64, i64 } poison, i64 %319, 0
  %322 = insertvalue { i64, i64 } %321, i64 %320, 1
  ret { i64, i64 } %322
}

; Function Attrs: nounwind memory(argmem: readwrite, inaccessiblemem: write)
define weak hidden i32 @tz_soft_cmp(ptr noundef readonly %0, ptr noundef readonly %1, i32 noundef %2) local_unnamed_addr #2 {
  %4 = alloca %struct.tzrt_format, align 8
  %5 = alloca %struct.tzrt_number, align 4
  %6 = alloca %struct.tzrt_number, align 4
  call void @llvm.lifetime.start.p0(i64 36, ptr nonnull %4) #9
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
  store i32 2, ptr %4, align 8, !tbaa !4, !alias.scope !70
  %8 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 11, ptr %8, align 4, !tbaa !12, !alias.scope !70
  %9 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 -24, ptr %9, align 8, !tbaa !13, !alias.scope !70
  %10 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 5, ptr %10, align 4, !tbaa !14, !alias.scope !70
  %11 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 10, ptr %11, align 8, !tbaa !15, !alias.scope !70
  %12 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 15, ptr %12, align 4, !tbaa !16, !alias.scope !70
  %13 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 16, ptr %13, align 8, !tbaa !17, !alias.scope !70
  %14 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 0, ptr %14, align 4, !tbaa !18, !alias.scope !70
  %15 = getelementptr inbounds i8, ptr %4, i64 32
  store i32 1, ptr %15, align 8, !tbaa !19, !alias.scope !70
  br label %83

16:                                               ; preds = %3
  store i32 2, ptr %4, align 8, !tbaa !4, !alias.scope !70
  %17 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 24, ptr %17, align 4, !tbaa !12, !alias.scope !70
  %18 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 -149, ptr %18, align 8, !tbaa !13, !alias.scope !70
  %19 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 104, ptr %19, align 4, !tbaa !14, !alias.scope !70
  %20 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 23, ptr %20, align 8, !tbaa !15, !alias.scope !70
  %21 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 127, ptr %21, align 4, !tbaa !16, !alias.scope !70
  %22 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 32, ptr %22, align 8, !tbaa !17, !alias.scope !70
  %23 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 0, ptr %23, align 4, !tbaa !18, !alias.scope !70
  %24 = getelementptr inbounds i8, ptr %4, i64 32
  store i32 1, ptr %24, align 8, !tbaa !19, !alias.scope !70
  br label %83

25:                                               ; preds = %3
  store i32 2, ptr %4, align 8, !tbaa !4, !alias.scope !70
  %26 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 53, ptr %26, align 4, !tbaa !12, !alias.scope !70
  %27 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 -1074, ptr %27, align 8, !tbaa !13, !alias.scope !70
  %28 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 971, ptr %28, align 4, !tbaa !14, !alias.scope !70
  %29 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 52, ptr %29, align 8, !tbaa !15, !alias.scope !70
  %30 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 1023, ptr %30, align 4, !tbaa !16, !alias.scope !70
  %31 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 64, ptr %31, align 8, !tbaa !17, !alias.scope !70
  %32 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 0, ptr %32, align 4, !tbaa !18, !alias.scope !70
  %33 = getelementptr inbounds i8, ptr %4, i64 32
  store i32 1, ptr %33, align 8, !tbaa !19, !alias.scope !70
  br label %83

34:                                               ; preds = %3
  store i32 2, ptr %4, align 8, !tbaa !4, !alias.scope !70
  %35 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 113, ptr %35, align 4, !tbaa !12, !alias.scope !70
  %36 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 -16494, ptr %36, align 8, !tbaa !13, !alias.scope !70
  %37 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 16271, ptr %37, align 4, !tbaa !14, !alias.scope !70
  %38 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 112, ptr %38, align 8, !tbaa !15, !alias.scope !70
  %39 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 16383, ptr %39, align 4, !tbaa !16, !alias.scope !70
  %40 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 128, ptr %40, align 8, !tbaa !17, !alias.scope !70
  %41 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 0, ptr %41, align 4, !tbaa !18, !alias.scope !70
  %42 = getelementptr inbounds i8, ptr %4, i64 32
  store i32 1, ptr %42, align 8, !tbaa !19, !alias.scope !70
  br label %83

43:                                               ; preds = %3
  store i32 10, ptr %4, align 8, !tbaa !4, !alias.scope !70
  %44 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 7, ptr %44, align 4, !tbaa !12, !alias.scope !70
  %45 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 -101, ptr %45, align 8, !tbaa !13, !alias.scope !70
  %46 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 90, ptr %46, align 4, !tbaa !14, !alias.scope !70
  %47 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 23, ptr %47, align 8, !tbaa !15, !alias.scope !70
  %48 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 101, ptr %48, align 4, !tbaa !16, !alias.scope !70
  %49 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 32, ptr %49, align 8, !tbaa !17, !alias.scope !70
  %50 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 0, ptr %50, align 4, !tbaa !18, !alias.scope !70
  %51 = getelementptr inbounds i8, ptr %4, i64 32
  store i32 1, ptr %51, align 8, !tbaa !19, !alias.scope !70
  br label %83

52:                                               ; preds = %3
  store i32 10, ptr %4, align 8, !tbaa !4, !alias.scope !70
  %53 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 16, ptr %53, align 4, !tbaa !12, !alias.scope !70
  %54 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 -398, ptr %54, align 8, !tbaa !13, !alias.scope !70
  %55 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 369, ptr %55, align 4, !tbaa !14, !alias.scope !70
  %56 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 53, ptr %56, align 8, !tbaa !15, !alias.scope !70
  %57 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 398, ptr %57, align 4, !tbaa !16, !alias.scope !70
  %58 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 64, ptr %58, align 8, !tbaa !17, !alias.scope !70
  %59 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 0, ptr %59, align 4, !tbaa !18, !alias.scope !70
  %60 = getelementptr inbounds i8, ptr %4, i64 32
  store i32 1, ptr %60, align 8, !tbaa !19, !alias.scope !70
  br label %83

61:                                               ; preds = %3
  store i32 10, ptr %4, align 8, !tbaa !4, !alias.scope !70
  %62 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 34, ptr %62, align 4, !tbaa !12, !alias.scope !70
  %63 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 -6176, ptr %63, align 8, !tbaa !13, !alias.scope !70
  %64 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 6111, ptr %64, align 4, !tbaa !14, !alias.scope !70
  %65 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 113, ptr %65, align 8, !tbaa !15, !alias.scope !70
  %66 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 6176, ptr %66, align 4, !tbaa !16, !alias.scope !70
  %67 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 128, ptr %67, align 8, !tbaa !17, !alias.scope !70
  %68 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 0, ptr %68, align 4, !tbaa !18, !alias.scope !70
  %69 = getelementptr inbounds i8, ptr %4, i64 32
  store i32 1, ptr %69, align 8, !tbaa !19, !alias.scope !70
  br label %83

70:                                               ; preds = %3
  %71 = and i32 %2, 7
  %72 = shl nuw nsw i32 8, %71
  store i32 2, ptr %4, align 8, !tbaa !4, !alias.scope !70
  %73 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 %72, ptr %73, align 4, !tbaa !12, !alias.scope !70
  %74 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 0, ptr %74, align 8, !tbaa !13, !alias.scope !70
  %75 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 0, ptr %75, align 4, !tbaa !14, !alias.scope !70
  %76 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 0, ptr %76, align 8, !tbaa !15, !alias.scope !70
  %77 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 0, ptr %77, align 4, !tbaa !16, !alias.scope !70
  %78 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 %72, ptr %78, align 8, !tbaa !17, !alias.scope !70
  %79 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 1, ptr %79, align 4, !tbaa !18, !alias.scope !70
  %80 = getelementptr inbounds i8, ptr %4, i64 32
  %81 = icmp slt i32 %2, 24
  %82 = zext i1 %81 to i32
  store i32 %82, ptr %80, align 8, !tbaa !19, !alias.scope !70
  br label %83

83:                                               ; preds = %7, %16, %25, %34, %43, %52, %61, %70
  %84 = phi i32 [ 2, %7 ], [ 2, %16 ], [ 2, %25 ], [ 2, %34 ], [ 10, %43 ], [ 10, %52 ], [ 10, %61 ], [ 2, %70 ]
  call void @llvm.lifetime.start.p0(i64 5616, ptr nonnull %5) #9
  call fastcc void @decode(ptr sret(%struct.tzrt_number) align 4 %5, ptr noundef %0, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %4) #10
  call void @llvm.lifetime.start.p0(i64 5616, ptr nonnull %6) #9
  call fastcc void @decode(ptr sret(%struct.tzrt_number) align 4 %6, ptr noundef %1, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %4) #10
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
  call fastcc void @power(ptr noundef %5, i32 noundef %84, i32 noundef %120) #10
  %121 = sub nsw i32 %118, %119
  call fastcc void @power(ptr noundef %6, i32 noundef %84, i32 noundef %121) #10
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
  call void @llvm.lifetime.end.p0(i64 5616, ptr nonnull %6) #9
  call void @llvm.lifetime.end.p0(i64 5616, ptr nonnull %5) #9
  call void @llvm.lifetime.end.p0(i64 36, ptr nonnull %4) #9
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
  call void @llvm.lifetime.start.p0(i64 36, ptr nonnull %5) #9
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
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !73
  %15 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 11, ptr %15, align 4, !tbaa !12, !alias.scope !73
  %16 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -24, ptr %16, align 8, !tbaa !13, !alias.scope !73
  %17 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 5, ptr %17, align 4, !tbaa !14, !alias.scope !73
  %18 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 10, ptr %18, align 8, !tbaa !15, !alias.scope !73
  %19 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 15, ptr %19, align 4, !tbaa !16, !alias.scope !73
  %20 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 16, ptr %20, align 8, !tbaa !17, !alias.scope !73
  %21 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %21, align 4, !tbaa !18, !alias.scope !73
  %22 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %22, align 8, !tbaa !19, !alias.scope !73
  br label %90

23:                                               ; preds = %4
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !73
  %24 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 24, ptr %24, align 4, !tbaa !12, !alias.scope !73
  %25 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -149, ptr %25, align 8, !tbaa !13, !alias.scope !73
  %26 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 104, ptr %26, align 4, !tbaa !14, !alias.scope !73
  %27 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 23, ptr %27, align 8, !tbaa !15, !alias.scope !73
  %28 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 127, ptr %28, align 4, !tbaa !16, !alias.scope !73
  %29 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 32, ptr %29, align 8, !tbaa !17, !alias.scope !73
  %30 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %30, align 4, !tbaa !18, !alias.scope !73
  %31 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %31, align 8, !tbaa !19, !alias.scope !73
  br label %90

32:                                               ; preds = %4
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !73
  %33 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 53, ptr %33, align 4, !tbaa !12, !alias.scope !73
  %34 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -1074, ptr %34, align 8, !tbaa !13, !alias.scope !73
  %35 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 971, ptr %35, align 4, !tbaa !14, !alias.scope !73
  %36 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 52, ptr %36, align 8, !tbaa !15, !alias.scope !73
  %37 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 1023, ptr %37, align 4, !tbaa !16, !alias.scope !73
  %38 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 64, ptr %38, align 8, !tbaa !17, !alias.scope !73
  %39 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %39, align 4, !tbaa !18, !alias.scope !73
  %40 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %40, align 8, !tbaa !19, !alias.scope !73
  br label %90

41:                                               ; preds = %4
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !73
  %42 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 113, ptr %42, align 4, !tbaa !12, !alias.scope !73
  %43 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -16494, ptr %43, align 8, !tbaa !13, !alias.scope !73
  %44 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 16271, ptr %44, align 4, !tbaa !14, !alias.scope !73
  %45 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 112, ptr %45, align 8, !tbaa !15, !alias.scope !73
  %46 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 16383, ptr %46, align 4, !tbaa !16, !alias.scope !73
  %47 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 128, ptr %47, align 8, !tbaa !17, !alias.scope !73
  %48 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %48, align 4, !tbaa !18, !alias.scope !73
  %49 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %49, align 8, !tbaa !19, !alias.scope !73
  br label %90

50:                                               ; preds = %4
  store i32 10, ptr %5, align 8, !tbaa !4, !alias.scope !73
  %51 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 7, ptr %51, align 4, !tbaa !12, !alias.scope !73
  %52 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -101, ptr %52, align 8, !tbaa !13, !alias.scope !73
  %53 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 90, ptr %53, align 4, !tbaa !14, !alias.scope !73
  %54 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 23, ptr %54, align 8, !tbaa !15, !alias.scope !73
  %55 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 101, ptr %55, align 4, !tbaa !16, !alias.scope !73
  %56 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 32, ptr %56, align 8, !tbaa !17, !alias.scope !73
  %57 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %57, align 4, !tbaa !18, !alias.scope !73
  %58 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %58, align 8, !tbaa !19, !alias.scope !73
  br label %90

59:                                               ; preds = %4
  store i32 10, ptr %5, align 8, !tbaa !4, !alias.scope !73
  %60 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 16, ptr %60, align 4, !tbaa !12, !alias.scope !73
  %61 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -398, ptr %61, align 8, !tbaa !13, !alias.scope !73
  %62 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 369, ptr %62, align 4, !tbaa !14, !alias.scope !73
  %63 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 53, ptr %63, align 8, !tbaa !15, !alias.scope !73
  %64 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 398, ptr %64, align 4, !tbaa !16, !alias.scope !73
  %65 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 64, ptr %65, align 8, !tbaa !17, !alias.scope !73
  %66 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %66, align 4, !tbaa !18, !alias.scope !73
  %67 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %67, align 8, !tbaa !19, !alias.scope !73
  br label %90

68:                                               ; preds = %4
  store i32 10, ptr %5, align 8, !tbaa !4, !alias.scope !73
  %69 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 34, ptr %69, align 4, !tbaa !12, !alias.scope !73
  %70 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -6176, ptr %70, align 8, !tbaa !13, !alias.scope !73
  %71 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 6111, ptr %71, align 4, !tbaa !14, !alias.scope !73
  %72 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 113, ptr %72, align 8, !tbaa !15, !alias.scope !73
  %73 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 6176, ptr %73, align 4, !tbaa !16, !alias.scope !73
  %74 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 128, ptr %74, align 8, !tbaa !17, !alias.scope !73
  %75 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %75, align 4, !tbaa !18, !alias.scope !73
  %76 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %76, align 8, !tbaa !19, !alias.scope !73
  br label %90

77:                                               ; preds = %4
  %78 = and i32 %2, 7
  %79 = shl nuw nsw i32 8, %78
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !73
  %80 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 %79, ptr %80, align 4, !tbaa !12, !alias.scope !73
  %81 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 0, ptr %81, align 8, !tbaa !13, !alias.scope !73
  %82 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 0, ptr %82, align 4, !tbaa !14, !alias.scope !73
  %83 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 0, ptr %83, align 8, !tbaa !15, !alias.scope !73
  %84 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 0, ptr %84, align 4, !tbaa !16, !alias.scope !73
  %85 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 %79, ptr %85, align 8, !tbaa !17, !alias.scope !73
  %86 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 1, ptr %86, align 4, !tbaa !18, !alias.scope !73
  %87 = getelementptr inbounds i8, ptr %5, i64 32
  %88 = icmp slt i32 %2, 24
  %89 = zext i1 %88 to i32
  store i32 %89, ptr %87, align 8, !tbaa !19, !alias.scope !73
  br label %90

90:                                               ; preds = %14, %23, %32, %41, %50, %59, %68, %77
  %91 = phi i32 [ 2, %14 ], [ 2, %23 ], [ 2, %32 ], [ 2, %41 ], [ 10, %50 ], [ 10, %59 ], [ 10, %68 ], [ 2, %77 ]
  call void @llvm.lifetime.start.p0(i64 36, ptr nonnull %6) #9
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
  store i32 2, ptr %6, align 8, !tbaa !4, !alias.scope !76
  %93 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 11, ptr %93, align 4, !tbaa !12, !alias.scope !76
  %94 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 -24, ptr %94, align 8, !tbaa !13, !alias.scope !76
  %95 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 5, ptr %95, align 4, !tbaa !14, !alias.scope !76
  %96 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 10, ptr %96, align 8, !tbaa !15, !alias.scope !76
  %97 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 15, ptr %97, align 4, !tbaa !16, !alias.scope !76
  %98 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 16, ptr %98, align 8, !tbaa !17, !alias.scope !76
  %99 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 0, ptr %99, align 4, !tbaa !18, !alias.scope !76
  %100 = getelementptr inbounds i8, ptr %6, i64 32
  store i32 1, ptr %100, align 8, !tbaa !19, !alias.scope !76
  br label %168

101:                                              ; preds = %90
  store i32 2, ptr %6, align 8, !tbaa !4, !alias.scope !76
  %102 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 24, ptr %102, align 4, !tbaa !12, !alias.scope !76
  %103 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 -149, ptr %103, align 8, !tbaa !13, !alias.scope !76
  %104 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 104, ptr %104, align 4, !tbaa !14, !alias.scope !76
  %105 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 23, ptr %105, align 8, !tbaa !15, !alias.scope !76
  %106 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 127, ptr %106, align 4, !tbaa !16, !alias.scope !76
  %107 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 32, ptr %107, align 8, !tbaa !17, !alias.scope !76
  %108 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 0, ptr %108, align 4, !tbaa !18, !alias.scope !76
  %109 = getelementptr inbounds i8, ptr %6, i64 32
  store i32 1, ptr %109, align 8, !tbaa !19, !alias.scope !76
  br label %168

110:                                              ; preds = %90
  store i32 2, ptr %6, align 8, !tbaa !4, !alias.scope !76
  %111 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 53, ptr %111, align 4, !tbaa !12, !alias.scope !76
  %112 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 -1074, ptr %112, align 8, !tbaa !13, !alias.scope !76
  %113 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 971, ptr %113, align 4, !tbaa !14, !alias.scope !76
  %114 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 52, ptr %114, align 8, !tbaa !15, !alias.scope !76
  %115 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 1023, ptr %115, align 4, !tbaa !16, !alias.scope !76
  %116 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 64, ptr %116, align 8, !tbaa !17, !alias.scope !76
  %117 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 0, ptr %117, align 4, !tbaa !18, !alias.scope !76
  %118 = getelementptr inbounds i8, ptr %6, i64 32
  store i32 1, ptr %118, align 8, !tbaa !19, !alias.scope !76
  br label %168

119:                                              ; preds = %90
  store i32 2, ptr %6, align 8, !tbaa !4, !alias.scope !76
  %120 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 113, ptr %120, align 4, !tbaa !12, !alias.scope !76
  %121 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 -16494, ptr %121, align 8, !tbaa !13, !alias.scope !76
  %122 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 16271, ptr %122, align 4, !tbaa !14, !alias.scope !76
  %123 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 112, ptr %123, align 8, !tbaa !15, !alias.scope !76
  %124 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 16383, ptr %124, align 4, !tbaa !16, !alias.scope !76
  %125 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 128, ptr %125, align 8, !tbaa !17, !alias.scope !76
  %126 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 0, ptr %126, align 4, !tbaa !18, !alias.scope !76
  %127 = getelementptr inbounds i8, ptr %6, i64 32
  store i32 1, ptr %127, align 8, !tbaa !19, !alias.scope !76
  br label %168

128:                                              ; preds = %90
  store i32 10, ptr %6, align 8, !tbaa !4, !alias.scope !76
  %129 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 7, ptr %129, align 4, !tbaa !12, !alias.scope !76
  %130 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 -101, ptr %130, align 8, !tbaa !13, !alias.scope !76
  %131 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 90, ptr %131, align 4, !tbaa !14, !alias.scope !76
  %132 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 23, ptr %132, align 8, !tbaa !15, !alias.scope !76
  %133 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 101, ptr %133, align 4, !tbaa !16, !alias.scope !76
  %134 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 32, ptr %134, align 8, !tbaa !17, !alias.scope !76
  %135 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 0, ptr %135, align 4, !tbaa !18, !alias.scope !76
  %136 = getelementptr inbounds i8, ptr %6, i64 32
  store i32 1, ptr %136, align 8, !tbaa !19, !alias.scope !76
  br label %168

137:                                              ; preds = %90
  store i32 10, ptr %6, align 8, !tbaa !4, !alias.scope !76
  %138 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 16, ptr %138, align 4, !tbaa !12, !alias.scope !76
  %139 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 -398, ptr %139, align 8, !tbaa !13, !alias.scope !76
  %140 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 369, ptr %140, align 4, !tbaa !14, !alias.scope !76
  %141 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 53, ptr %141, align 8, !tbaa !15, !alias.scope !76
  %142 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 398, ptr %142, align 4, !tbaa !16, !alias.scope !76
  %143 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 64, ptr %143, align 8, !tbaa !17, !alias.scope !76
  %144 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 0, ptr %144, align 4, !tbaa !18, !alias.scope !76
  %145 = getelementptr inbounds i8, ptr %6, i64 32
  store i32 1, ptr %145, align 8, !tbaa !19, !alias.scope !76
  br label %168

146:                                              ; preds = %90
  store i32 10, ptr %6, align 8, !tbaa !4, !alias.scope !76
  %147 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 34, ptr %147, align 4, !tbaa !12, !alias.scope !76
  %148 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 -6176, ptr %148, align 8, !tbaa !13, !alias.scope !76
  %149 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 6111, ptr %149, align 4, !tbaa !14, !alias.scope !76
  %150 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 113, ptr %150, align 8, !tbaa !15, !alias.scope !76
  %151 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 6176, ptr %151, align 4, !tbaa !16, !alias.scope !76
  %152 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 128, ptr %152, align 8, !tbaa !17, !alias.scope !76
  %153 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 0, ptr %153, align 4, !tbaa !18, !alias.scope !76
  %154 = getelementptr inbounds i8, ptr %6, i64 32
  store i32 1, ptr %154, align 8, !tbaa !19, !alias.scope !76
  br label %168

155:                                              ; preds = %90
  %156 = and i32 %3, 7
  %157 = shl nuw nsw i32 8, %156
  store i32 2, ptr %6, align 8, !tbaa !4, !alias.scope !76
  %158 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 %157, ptr %158, align 4, !tbaa !12, !alias.scope !76
  %159 = getelementptr inbounds i8, ptr %6, i64 8
  store i32 0, ptr %159, align 8, !tbaa !13, !alias.scope !76
  %160 = getelementptr inbounds i8, ptr %6, i64 12
  store i32 0, ptr %160, align 4, !tbaa !14, !alias.scope !76
  %161 = getelementptr inbounds i8, ptr %6, i64 16
  store i32 0, ptr %161, align 8, !tbaa !15, !alias.scope !76
  %162 = getelementptr inbounds i8, ptr %6, i64 20
  store i32 0, ptr %162, align 4, !tbaa !16, !alias.scope !76
  %163 = getelementptr inbounds i8, ptr %6, i64 24
  store i32 %157, ptr %163, align 8, !tbaa !17, !alias.scope !76
  %164 = getelementptr inbounds i8, ptr %6, i64 28
  store i32 1, ptr %164, align 4, !tbaa !18, !alias.scope !76
  %165 = getelementptr inbounds i8, ptr %6, i64 32
  %166 = icmp slt i32 %3, 24
  %167 = zext i1 %166 to i32
  store i32 %167, ptr %165, align 8, !tbaa !19, !alias.scope !76
  br label %168

168:                                              ; preds = %92, %101, %110, %119, %128, %137, %146, %155
  %169 = phi i32 [ 1, %92 ], [ 1, %101 ], [ 1, %110 ], [ 1, %119 ], [ 1, %128 ], [ 1, %137 ], [ 1, %146 ], [ %167, %155 ]
  %170 = phi i1 [ true, %92 ], [ true, %101 ], [ true, %110 ], [ true, %119 ], [ true, %128 ], [ true, %137 ], [ true, %146 ], [ false, %155 ]
  %171 = phi i32 [ 16, %92 ], [ 32, %101 ], [ 64, %110 ], [ 128, %119 ], [ 32, %128 ], [ 64, %137 ], [ 128, %146 ], [ %157, %155 ]
  %172 = phi i32 [ 31, %92 ], [ 255, %101 ], [ 2047, %110 ], [ 32767, %119 ], [ 203, %128 ], [ 797, %137 ], [ 12353, %146 ], [ 1, %155 ]
  %173 = phi i32 [ 10, %92 ], [ 23, %101 ], [ 52, %110 ], [ 112, %119 ], [ 23, %128 ], [ 53, %137 ], [ 113, %146 ], [ 0, %155 ]
  %174 = phi i1 [ true, %92 ], [ true, %101 ], [ true, %110 ], [ true, %119 ], [ false, %128 ], [ false, %137 ], [ false, %146 ], [ true, %155 ]
  %175 = phi i32 [ 2, %92 ], [ 2, %101 ], [ 2, %110 ], [ 2, %119 ], [ 10, %128 ], [ 10, %137 ], [ 10, %146 ], [ 2, %155 ]
  call void @llvm.lifetime.start.p0(i64 5616, ptr nonnull %7) #9
  call fastcc void @decode(ptr sret(%struct.tzrt_number) align 4 %7, ptr noundef %1, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %5) #10
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
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %8) #9
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %8, ptr noundef nonnull align 4 dereferenceable(5604) %7, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %9) #9
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %9, i8 0, i64 5604, i1 false), !alias.scope !79
  %213 = getelementptr inbounds i8, ptr %9, i64 4
  store i32 1, ptr %9, align 4, !tbaa !34, !alias.scope !79
  store i32 1, ptr %213, align 4, !tbaa !24, !alias.scope !79
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %10) #9
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %10, i8 0, i64 5604, i1 false), !alias.scope !82
  %214 = icmp eq i128 %196, 0
  br i1 %214, label %226, label %215

215:                                              ; preds = %212
  %216 = getelementptr inbounds i8, ptr %10, i64 4
  br label %217

217:                                              ; preds = %217, %215
  %218 = phi i128 [ %196, %215 ], [ %224, %217 ]
  %219 = trunc i128 %218 to i32
  %220 = load i32, ptr %10, align 4, !tbaa !34, !alias.scope !82
  %221 = add nsw i32 %220, 1
  store i32 %221, ptr %10, align 4, !tbaa !34, !alias.scope !82
  %222 = sext i32 %220 to i64
  %223 = getelementptr inbounds [1400 x i32], ptr %216, i64 0, i64 %222
  store i32 %219, ptr %223, align 4, !tbaa !24, !alias.scope !82
  %224 = lshr i128 %218, 32
  %225 = icmp ult i128 %218, 4294967296
  br i1 %225, label %226, label %217
226:                                              ; preds = %217, %212
  %227 = getelementptr inbounds i8, ptr %7, i64 5604
  %228 = load i32, ptr %227, align 4, !tbaa !35
  %229 = icmp sgt i32 %228, -1
  br i1 %229, label %230, label %231

230:                                              ; preds = %226
  call fastcc void @power(ptr noundef %8, i32 noundef %91, i32 noundef %228) #10
  br label %233

231:                                              ; preds = %226
  %232 = sub nsw i32 0, %228
  call fastcc void @power(ptr noundef %9, i32 noundef %91, i32 noundef %232) #10
  br label %233

233:                                              ; preds = %231, %230
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %11) #9
  tail call void @llvm.experimental.noalias.scope.decl(metadata !85)
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %11, i8 0, i64 5604, i1 false), !alias.scope !85
  %234 = load i32, ptr %10, align 4, !tbaa !34, !noalias !85
  %235 = load i32, ptr %9, align 4, !tbaa !34, !noalias !85
  %236 = add nsw i32 %235, %234
  %237 = icmp sgt i32 %236, 1400
  br i1 %237, label %238, label %239

238:                                              ; preds = %233
  tail call void @llvm.trap()
  unreachable

239:                                              ; preds = %233
  store i32 %236, ptr %11, align 4, !tbaa !34, !alias.scope !85
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
  %252 = load i32, ptr %251, align 4, !tbaa !24, !noalias !85
  %253 = zext i32 %252 to i64
  br label %277

254:                                              ; preds = %271, %239
  %255 = getelementptr inbounds i8, ptr %11, i64 4
  %256 = load i32, ptr %11, align 4, !tbaa !34, !alias.scope !85
  %257 = icmp eq i32 %256, 0
  br i1 %257, label %294, label %258

258:                                              ; preds = %254
  %259 = sext i32 %256 to i64
  br label %260

260:                                              ; preds = %266, %258
  %261 = phi i64 [ %259, %258 ], [ %262, %266 ]
  %262 = add nsw i64 %261, -1
  %263 = getelementptr inbounds [1400 x i32], ptr %255, i64 0, i64 %262
  %264 = load i32, ptr %263, align 4, !tbaa !24, !alias.scope !85
  %265 = icmp eq i32 %264, 0
  br i1 %265, label %266, label %294

266:                                              ; preds = %260
  %267 = trunc i64 %262 to i32
  store i32 %267, ptr %11, align 4, !tbaa !34, !alias.scope !85
  %268 = icmp eq i64 %262, 0
  br i1 %268, label %294, label %260
269:                                              ; preds = %277
  %270 = trunc i64 %291 to i32
  br label %271

271:                                              ; preds = %269, %248
  %272 = phi i32 [ 0, %248 ], [ %270, %269 ]
  %273 = add nsw i64 %249, %245
  %274 = getelementptr inbounds [1400 x i32], ptr %244, i64 0, i64 %273
  store i32 %272, ptr %274, align 4, !tbaa !24, !alias.scope !85
  %275 = add nuw nsw i64 %249, 1
  %276 = icmp eq i64 %275, %246
  br i1 %276, label %254, label %248
277:                                              ; preds = %277, %250
  %278 = phi i64 [ 0, %250 ], [ %292, %277 ]
  %279 = phi i64 [ 0, %250 ], [ %291, %277 ]
  %280 = getelementptr inbounds [1400 x i32], ptr %213, i64 0, i64 %278
  %281 = load i32, ptr %280, align 4, !tbaa !24, !noalias !85
  %282 = zext i32 %281 to i64
  %283 = mul nuw i64 %282, %253
  %284 = add nuw nsw i64 %278, %249
  %285 = getelementptr inbounds [1400 x i32], ptr %244, i64 0, i64 %284
  %286 = load i32, ptr %285, align 4, !tbaa !24, !alias.scope !85
  %287 = zext i32 %286 to i64
  %288 = add nuw nsw i64 %279, %287
  %289 = add nuw i64 %288, %283
  %290 = trunc i64 %289 to i32
  store i32 %290, ptr %285, align 4, !tbaa !24, !alias.scope !85
  %291 = lshr i64 %289, 32
  %292 = add nuw nsw i64 %278, 1
  %293 = icmp eq i64 %292, %247
  br i1 %293, label %269, label %277
294:                                              ; preds = %260, %266, %254
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %10, ptr noundef nonnull align 4 dereferenceable(5604) %11, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %11) #9
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
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %12) #9
  call fastcc void @divide(ptr sret(%struct.tzrt_big) align 4 %12, ptr noundef %8, ptr noundef %9) #10
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
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %12) #9
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
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %10) #9
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %9) #9
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %8) #9
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
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %13) #9
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %13, i8 0, i64 5604, i1 false), !alias.scope !88
  %426 = getelementptr inbounds i8, ptr %13, i64 4
  store i32 1, ptr %13, align 4, !tbaa !34, !alias.scope !88
  store i32 1, ptr %426, align 4, !tbaa !24, !alias.scope !88
  %427 = icmp eq i32 %91, %175
  br i1 %427, label %436, label %428

428:                                              ; preds = %425
  %429 = getelementptr inbounds i8, ptr %7, i64 5604
  %430 = load i32, ptr %429, align 4, !tbaa !35
  %431 = icmp sgt i32 %430, -1
  br i1 %431, label %432, label %433

432:                                              ; preds = %428
  call fastcc void @power(ptr noundef %7, i32 noundef %91, i32 noundef %430) #10
  br label %435

433:                                              ; preds = %428
  %434 = sub nsw i32 0, %430
  call fastcc void @power(ptr noundef %13, i32 noundef %91, i32 noundef %434) #10
  br label %435

435:                                              ; preds = %433, %432
  store i32 0, ptr %429, align 4, !tbaa !35
  br label %436

436:                                              ; preds = %435, %425
  %437 = getelementptr inbounds i8, ptr %7, i64 5604
  %438 = load i32, ptr %437, align 4, !tbaa !35
  %439 = getelementptr inbounds i8, ptr %7, i64 5608
  %440 = load i32, ptr %439, align 4, !tbaa !20
  %441 = call fastcc { i64, i64 } @pack(ptr noundef %7, ptr noundef %13, i32 noundef %438, i32 noundef %440, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %6) #10
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
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %13) #9
  br label %459

459:                                              ; preds = %204, %417, %368, %458
  call void @llvm.lifetime.end.p0(i64 5616, ptr nonnull %7) #9
  call void @llvm.lifetime.end.p0(i64 36, ptr nonnull %6) #9
  call void @llvm.lifetime.end.p0(i64 36, ptr nonnull %5) #9
  ret void
}

; Function Attrs: nounwind memory(argmem: readwrite, inaccessiblemem: write)
define internal fastcc void @divide(ptr noalias nonnull sret(%struct.tzrt_big) align 4 %0, ptr noundef nonnull %1, ptr noundef nonnull readonly %2) unnamed_addr #2 {
  %4 = alloca %struct.tzrt_big, align 4
  tail call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %0, i8 0, i64 5604, i1 false)
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %4) #9
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
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %4) #9
  ret void
}

; Function Attrs: nounwind memory(argmem: readwrite, inaccessiblemem: write)
define weak hidden i32 @tz_soft_format(ptr noundef writeonly %0, ptr noundef readonly %1, i32 noundef %2) local_unnamed_addr #2 {
  %4 = alloca [12 x i8], align 1
  %5 = alloca %struct.tzrt_format, align 8
  %6 = alloca %struct.tzrt_number, align 4
  %7 = alloca %struct.tzrt_big, align 4
  %8 = alloca %struct.tzrt_big, align 4
  %9 = alloca [48 x i8], align 16
  call void @llvm.lifetime.start.p0(i64 36, ptr nonnull %5) #9
  switch i32 %2, label %73 [
    i32 0, label %10
    i32 1, label %19
    i32 2, label %28
    i32 3, label %37
    i32 4, label %46
    i32 5, label %55
    i32 6, label %64
  ]

10:                                               ; preds = %3
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !93
  %11 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 11, ptr %11, align 4, !tbaa !12, !alias.scope !93
  %12 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -24, ptr %12, align 8, !tbaa !13, !alias.scope !93
  %13 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 5, ptr %13, align 4, !tbaa !14, !alias.scope !93
  %14 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 10, ptr %14, align 8, !tbaa !15, !alias.scope !93
  %15 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 15, ptr %15, align 4, !tbaa !16, !alias.scope !93
  %16 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 16, ptr %16, align 8, !tbaa !17, !alias.scope !93
  %17 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %17, align 4, !tbaa !18, !alias.scope !93
  %18 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %18, align 8, !tbaa !19, !alias.scope !93
  br label %86

19:                                               ; preds = %3
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !93
  %20 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 24, ptr %20, align 4, !tbaa !12, !alias.scope !93
  %21 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -149, ptr %21, align 8, !tbaa !13, !alias.scope !93
  %22 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 104, ptr %22, align 4, !tbaa !14, !alias.scope !93
  %23 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 23, ptr %23, align 8, !tbaa !15, !alias.scope !93
  %24 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 127, ptr %24, align 4, !tbaa !16, !alias.scope !93
  %25 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 32, ptr %25, align 8, !tbaa !17, !alias.scope !93
  %26 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %26, align 4, !tbaa !18, !alias.scope !93
  %27 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %27, align 8, !tbaa !19, !alias.scope !93
  br label %86

28:                                               ; preds = %3
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !93
  %29 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 53, ptr %29, align 4, !tbaa !12, !alias.scope !93
  %30 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -1074, ptr %30, align 8, !tbaa !13, !alias.scope !93
  %31 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 971, ptr %31, align 4, !tbaa !14, !alias.scope !93
  %32 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 52, ptr %32, align 8, !tbaa !15, !alias.scope !93
  %33 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 1023, ptr %33, align 4, !tbaa !16, !alias.scope !93
  %34 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 64, ptr %34, align 8, !tbaa !17, !alias.scope !93
  %35 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %35, align 4, !tbaa !18, !alias.scope !93
  %36 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %36, align 8, !tbaa !19, !alias.scope !93
  br label %86

37:                                               ; preds = %3
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !93
  %38 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 113, ptr %38, align 4, !tbaa !12, !alias.scope !93
  %39 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -16494, ptr %39, align 8, !tbaa !13, !alias.scope !93
  %40 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 16271, ptr %40, align 4, !tbaa !14, !alias.scope !93
  %41 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 112, ptr %41, align 8, !tbaa !15, !alias.scope !93
  %42 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 16383, ptr %42, align 4, !tbaa !16, !alias.scope !93
  %43 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 128, ptr %43, align 8, !tbaa !17, !alias.scope !93
  %44 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %44, align 4, !tbaa !18, !alias.scope !93
  %45 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %45, align 8, !tbaa !19, !alias.scope !93
  br label %86

46:                                               ; preds = %3
  store i32 10, ptr %5, align 8, !tbaa !4, !alias.scope !93
  %47 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 7, ptr %47, align 4, !tbaa !12, !alias.scope !93
  %48 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -101, ptr %48, align 8, !tbaa !13, !alias.scope !93
  %49 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 90, ptr %49, align 4, !tbaa !14, !alias.scope !93
  %50 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 23, ptr %50, align 8, !tbaa !15, !alias.scope !93
  %51 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 101, ptr %51, align 4, !tbaa !16, !alias.scope !93
  %52 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 32, ptr %52, align 8, !tbaa !17, !alias.scope !93
  %53 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %53, align 4, !tbaa !18, !alias.scope !93
  %54 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %54, align 8, !tbaa !19, !alias.scope !93
  br label %86

55:                                               ; preds = %3
  store i32 10, ptr %5, align 8, !tbaa !4, !alias.scope !93
  %56 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 16, ptr %56, align 4, !tbaa !12, !alias.scope !93
  %57 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -398, ptr %57, align 8, !tbaa !13, !alias.scope !93
  %58 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 369, ptr %58, align 4, !tbaa !14, !alias.scope !93
  %59 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 53, ptr %59, align 8, !tbaa !15, !alias.scope !93
  %60 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 398, ptr %60, align 4, !tbaa !16, !alias.scope !93
  %61 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 64, ptr %61, align 8, !tbaa !17, !alias.scope !93
  %62 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %62, align 4, !tbaa !18, !alias.scope !93
  %63 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %63, align 8, !tbaa !19, !alias.scope !93
  br label %86

64:                                               ; preds = %3
  store i32 10, ptr %5, align 8, !tbaa !4, !alias.scope !93
  %65 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 34, ptr %65, align 4, !tbaa !12, !alias.scope !93
  %66 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -6176, ptr %66, align 8, !tbaa !13, !alias.scope !93
  %67 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 6111, ptr %67, align 4, !tbaa !14, !alias.scope !93
  %68 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 113, ptr %68, align 8, !tbaa !15, !alias.scope !93
  %69 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 6176, ptr %69, align 4, !tbaa !16, !alias.scope !93
  %70 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 128, ptr %70, align 8, !tbaa !17, !alias.scope !93
  %71 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %71, align 4, !tbaa !18, !alias.scope !93
  %72 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %72, align 8, !tbaa !19, !alias.scope !93
  br label %86

73:                                               ; preds = %3
  %74 = and i32 %2, 7
  %75 = shl nuw nsw i32 8, %74
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !93
  %76 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 %75, ptr %76, align 4, !tbaa !12, !alias.scope !93
  %77 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 0, ptr %77, align 8, !tbaa !13, !alias.scope !93
  %78 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 0, ptr %78, align 4, !tbaa !14, !alias.scope !93
  %79 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 0, ptr %79, align 8, !tbaa !15, !alias.scope !93
  %80 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 0, ptr %80, align 4, !tbaa !16, !alias.scope !93
  %81 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 %75, ptr %81, align 8, !tbaa !17, !alias.scope !93
  %82 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 1, ptr %82, align 4, !tbaa !18, !alias.scope !93
  %83 = getelementptr inbounds i8, ptr %5, i64 32
  %84 = icmp slt i32 %2, 24
  %85 = zext i1 %84 to i32
  store i32 %85, ptr %83, align 8, !tbaa !19, !alias.scope !93
  br label %86

86:                                               ; preds = %10, %19, %28, %37, %46, %55, %64, %73
  %87 = phi i1 [ false, %10 ], [ false, %19 ], [ false, %28 ], [ false, %37 ], [ false, %46 ], [ false, %55 ], [ false, %64 ], [ true, %73 ]
  %88 = phi i1 [ true, %10 ], [ true, %19 ], [ true, %28 ], [ true, %37 ], [ true, %46 ], [ true, %55 ], [ true, %64 ], [ false, %73 ]
  %89 = phi i1 [ false, %10 ], [ false, %19 ], [ false, %28 ], [ false, %37 ], [ true, %46 ], [ true, %55 ], [ true, %64 ], [ false, %73 ]
  call void @llvm.lifetime.start.p0(i64 5616, ptr nonnull %6) #9
  call fastcc void @decode(ptr sret(%struct.tzrt_number) align 4 %6, ptr noundef %1, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %5) #10
  %90 = getelementptr inbounds i8, ptr %6, i64 5608
  %91 = load i32, ptr %90, align 4, !tbaa !20
  %92 = icmp ne i32 %91, 0
  %93 = getelementptr inbounds i8, ptr %6, i64 5612
  %94 = load i32, ptr %93, align 4
  %95 = icmp ne i32 %94, 2
  %96 = select i1 %92, i1 %95, i1 false
  br i1 %96, label %97, label %98

97:                                               ; preds = %86
  store i8 45, ptr %0, align 1, !tbaa !26
  br label %98

98:                                               ; preds = %97, %86
  %99 = phi i32 [ 1, %97 ], [ 0, %86 ]
  %100 = icmp eq i32 %94, 0
  br i1 %100, label %114, label %101

101:                                              ; preds = %98
  %102 = icmp eq i32 %94, 2
  %103 = select i1 %102, ptr @.str, ptr @.str.1
  %104 = zext i32 %99 to i64
  br label %105

105:                                              ; preds = %101, %105
  %106 = phi i64 [ 0, %101 ], [ %112, %105 ]
  %107 = phi i64 [ %104, %101 ], [ %110, %105 ]
  %108 = getelementptr inbounds i8, ptr %103, i64 %106
  %109 = load i8, ptr %108, align 1, !tbaa !26
  %110 = add nuw nsw i64 %107, 1
  %111 = getelementptr inbounds i8, ptr %0, i64 %107
  store i8 %109, ptr %111, align 1, !tbaa !26
  %112 = add nuw nsw i64 %106, 1
  %113 = icmp eq i64 %112, 3
  br i1 %113, label %454, label %105
114:                                              ; preds = %98
  %115 = load i32, ptr %6, align 4, !tbaa !25
  %116 = icmp eq i32 %115, 0
  br i1 %116, label %117, label %121

117:                                              ; preds = %114
  %118 = add nuw nsw i32 %99, 1
  %119 = zext i32 %99 to i64
  %120 = getelementptr inbounds i8, ptr %0, i64 %119
  store i8 48, ptr %120, align 1, !tbaa !26
  br label %456

121:                                              ; preds = %114
  %122 = or i1 %87, %89
  br i1 %122, label %240, label %123

123:                                              ; preds = %121
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %7) #9
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %7, i8 0, i64 5604, i1 false), !alias.scope !97
  %124 = getelementptr inbounds i8, ptr %7, i64 4
  store i32 1, ptr %7, align 4, !tbaa !34, !alias.scope !97
  store i32 1, ptr %124, align 4, !tbaa !24, !alias.scope !97
  %125 = getelementptr inbounds i8, ptr %6, i64 5604
  %126 = load i32, ptr %125, align 4, !tbaa !35
  %127 = icmp sgt i32 %126, -1
  br i1 %127, label %128, label %177

128:                                              ; preds = %123
  %129 = icmp eq i32 %126, 0
  br i1 %129, label %236, label %130

130:                                              ; preds = %128
  %131 = lshr i32 %126, 5
  %132 = and i32 %126, 31
  %133 = add nsw i32 %131, %115
  %134 = icmp sgt i32 %133, 1399
  br i1 %134, label %139, label %135

135:                                              ; preds = %130
  %136 = getelementptr inbounds i8, ptr %6, i64 4
  %137 = sext i32 %115 to i64
  %138 = zext i32 %131 to i64
  br label %142

139:                                              ; preds = %130
  tail call void @llvm.trap()
  unreachable

140:                                              ; preds = %142
  %141 = icmp sgt i32 %126, 31
  br i1 %141, label %157, label %150

142:                                              ; preds = %142, %135
  %143 = phi i64 [ %137, %135 ], [ %144, %142 ]
  %144 = add nsw i64 %143, -1
  %145 = getelementptr inbounds [1400 x i32], ptr %136, i64 0, i64 %144
  %146 = load i32, ptr %145, align 4, !tbaa !24
  %147 = add nsw i64 %144, %138
  %148 = getelementptr inbounds [1400 x i32], ptr %136, i64 0, i64 %147
  store i32 %146, ptr %148, align 4, !tbaa !24
  %149 = icmp eq i64 %144, 0
  br i1 %149, label %140, label %142
150:                                              ; preds = %157, %140
  %151 = load i32, ptr %6, align 4, !tbaa !34
  %152 = add nsw i32 %151, %131
  store i32 %152, ptr %6, align 4, !tbaa !34
  %153 = icmp sgt i32 %151, 0
  br i1 %153, label %154, label %162

154:                                              ; preds = %150
  %155 = zext i32 %132 to i64
  %156 = zext i32 %152 to i64
  br label %165

157:                                              ; preds = %140, %157
  %158 = phi i64 [ %160, %157 ], [ 0, %140 ]
  %159 = getelementptr inbounds [1400 x i32], ptr %136, i64 0, i64 %158
  store i32 0, ptr %159, align 4, !tbaa !24
  %160 = add nuw nsw i64 %158, 1
  %161 = icmp eq i64 %160, %138
  br i1 %161, label %150, label %157
162:                                              ; preds = %165, %150
  %163 = phi i64 [ 0, %150 ], [ %174, %165 ]
  %164 = icmp eq i64 %163, 0
  br i1 %164, label %236, label %227

165:                                              ; preds = %165, %154
  %166 = phi i64 [ %138, %154 ], [ %175, %165 ]
  %167 = phi i64 [ 0, %154 ], [ %174, %165 ]
  %168 = getelementptr inbounds [1400 x i32], ptr %136, i64 0, i64 %166
  %169 = load i32, ptr %168, align 4, !tbaa !24
  %170 = zext i32 %169 to i64
  %171 = shl nuw nsw i64 %170, %155
  %172 = or i64 %171, %167
  %173 = trunc i64 %172 to i32
  store i32 %173, ptr %168, align 4, !tbaa !24
  %174 = lshr i64 %171, 32
  %175 = add nuw nsw i64 %166, 1
  %176 = icmp samesign ult i64 %175, %156
  br i1 %176, label %165, label %162
177:                                              ; preds = %123
  %178 = load i32, ptr %7, align 4, !tbaa !34
  %179 = icmp eq i32 %178, 0
  br i1 %179, label %236, label %180

180:                                              ; preds = %177
  %181 = sub nsw i32 0, %126
  %182 = sdiv i32 %126, -32
  %183 = and i32 %181, 31
  %184 = add nsw i32 %178, %182
  %185 = icmp sgt i32 %184, 1399
  br i1 %185, label %189, label %186

186:                                              ; preds = %180
  %187 = sext i32 %178 to i64
  %188 = zext i32 %182 to i64
  br label %192

189:                                              ; preds = %180
  tail call void @llvm.trap()
  unreachable

190:                                              ; preds = %192
  %191 = icmp slt i32 %126, -31
  br i1 %191, label %207, label %200

192:                                              ; preds = %192, %186
  %193 = phi i64 [ %187, %186 ], [ %194, %192 ]
  %194 = add nsw i64 %193, -1
  %195 = getelementptr inbounds [1400 x i32], ptr %124, i64 0, i64 %194
  %196 = load i32, ptr %195, align 4, !tbaa !24
  %197 = add nsw i64 %194, %188
  %198 = getelementptr inbounds [1400 x i32], ptr %124, i64 0, i64 %197
  store i32 %196, ptr %198, align 4, !tbaa !24
  %199 = icmp eq i64 %194, 0
  br i1 %199, label %190, label %192
200:                                              ; preds = %207, %190
  %201 = load i32, ptr %7, align 4, !tbaa !34
  %202 = add nsw i32 %201, %182
  store i32 %202, ptr %7, align 4, !tbaa !34
  %203 = icmp sgt i32 %201, 0
  br i1 %203, label %204, label %212

204:                                              ; preds = %200
  %205 = zext i32 %183 to i64
  %206 = zext i32 %202 to i64
  br label %215

207:                                              ; preds = %190, %207
  %208 = phi i64 [ %210, %207 ], [ 0, %190 ]
  %209 = getelementptr inbounds [1400 x i32], ptr %124, i64 0, i64 %208
  store i32 0, ptr %209, align 4, !tbaa !24
  %210 = add nuw nsw i64 %208, 1
  %211 = icmp eq i64 %210, %188
  br i1 %211, label %200, label %207
212:                                              ; preds = %215, %200
  %213 = phi i64 [ 0, %200 ], [ %224, %215 ]
  %214 = icmp eq i64 %213, 0
  br i1 %214, label %236, label %227

215:                                              ; preds = %215, %204
  %216 = phi i64 [ %188, %204 ], [ %225, %215 ]
  %217 = phi i64 [ 0, %204 ], [ %224, %215 ]
  %218 = getelementptr inbounds [1400 x i32], ptr %124, i64 0, i64 %216
  %219 = load i32, ptr %218, align 4, !tbaa !24
  %220 = zext i32 %219 to i64
  %221 = shl nuw nsw i64 %220, %205
  %222 = or i64 %221, %217
  %223 = trunc i64 %222 to i32
  store i32 %223, ptr %218, align 4, !tbaa !24
  %224 = lshr i64 %221, 32
  %225 = add nuw nsw i64 %216, 1
  %226 = icmp samesign ult i64 %225, %206
  br i1 %226, label %215, label %212
227:                                              ; preds = %212, %162
  %228 = phi i64 [ %163, %162 ], [ %213, %212 ]
  %229 = phi i32 [ %152, %162 ], [ %202, %212 ]
  %230 = phi ptr [ %6, %162 ], [ %7, %212 ]
  %231 = getelementptr inbounds i8, ptr %230, i64 4
  %232 = trunc i64 %228 to i32
  %233 = add nsw i32 %229, 1
  store i32 %233, ptr %230, align 4, !tbaa !34
  %234 = sext i32 %229 to i64
  %235 = getelementptr inbounds [1400 x i32], ptr %231, i64 0, i64 %234
  store i32 %232, ptr %235, align 4, !tbaa !24
  br label %236

236:                                              ; preds = %227, %212, %177, %162, %128
  %237 = call fastcc i32 @magnitude(ptr noundef %6, ptr noundef %7, i32 noundef 10) #10
  %238 = add nsw i32 %237, -35
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %8) #9
  %239 = sub nsw i32 35, %237
  call fastcc void @rounded(ptr sret(%struct.tzrt_big) align 4 %8, ptr noundef %6, ptr noundef %7, i32 noundef 10, i32 noundef %239) #10
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %6, ptr noundef nonnull align 4 dereferenceable(5604) %8, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %8) #9
  store i32 %238, ptr %125, align 4, !tbaa !35
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %7) #9
  br label %240

240:                                              ; preds = %236, %121
  call void @llvm.lifetime.start.p0(i64 48, ptr nonnull %9) #9
  %241 = getelementptr inbounds i8, ptr %6, i64 4
  br label %242

242:                                              ; preds = %280, %240
  %243 = phi i32 [ %287, %280 ], [ -1, %240 ]
  %244 = phi i32 [ %286, %280 ], [ -1, %240 ]
  %245 = phi i32 [ %285, %280 ], [ 0, %240 ]
  %246 = phi i64 [ %281, %280 ], [ 0, %240 ]
  %247 = load i32, ptr %6, align 4, !tbaa !34
  %248 = icmp eq i32 %247, 0
  br i1 %248, label %254, label %249

249:                                              ; preds = %242
  %250 = sext i32 %247 to i64
  br label %267

251:                                              ; preds = %267
  %252 = trunc i64 %278 to i8
  %253 = or i8 %252, 48
  br label %254

254:                                              ; preds = %251, %242
  %255 = phi i8 [ 48, %242 ], [ %253, %251 ]
  br i1 %248, label %280, label %256

256:                                              ; preds = %254
  %257 = sext i32 %247 to i64
  br label %258

258:                                              ; preds = %264, %256
  %259 = phi i64 [ %257, %256 ], [ %260, %264 ]
  %260 = add nsw i64 %259, -1
  %261 = getelementptr inbounds [1400 x i32], ptr %241, i64 0, i64 %260
  %262 = load i32, ptr %261, align 4, !tbaa !24
  %263 = icmp eq i32 %262, 0
  br i1 %263, label %264, label %280

264:                                              ; preds = %258
  %265 = trunc i64 %260 to i32
  store i32 %265, ptr %6, align 4, !tbaa !34
  %266 = icmp eq i64 %260, 0
  br i1 %266, label %280, label %258
267:                                              ; preds = %267, %249
  %268 = phi i64 [ %250, %249 ], [ %270, %267 ]
  %269 = phi i64 [ 0, %249 ], [ %278, %267 ]
  %270 = add nsw i64 %268, -1
  %271 = shl nuw nsw i64 %269, 32
  %272 = getelementptr inbounds [1400 x i32], ptr %241, i64 0, i64 %270
  %273 = load i32, ptr %272, align 4, !tbaa !24
  %274 = zext i32 %273 to i64
  %275 = or i64 %271, %274
  %276 = udiv i64 %275, 10
  %277 = trunc i64 %276 to i32
  store i32 %277, ptr %272, align 4, !tbaa !24
  %278 = urem i64 %275, 10
  %279 = icmp eq i64 %270, 0
  br i1 %279, label %251, label %267
280:                                              ; preds = %258, %264, %254
  %281 = add nuw nsw i64 %246, 1
  %282 = getelementptr inbounds [48 x i8], ptr %9, i64 0, i64 %246
  store i8 %255, ptr %282, align 1, !tbaa !26
  %283 = load i32, ptr %6, align 4, !tbaa !25
  %284 = icmp eq i32 %283, 0
  %285 = add nuw i32 %245, 1
  %286 = add i32 %244, 1
  %287 = add i32 %243, -1
  br i1 %284, label %288, label %242
288:                                              ; preds = %280
  %289 = trunc i64 %246 to i32
  %290 = trunc i64 %281 to i32
  %291 = icmp ne i64 %246, 0
  %292 = select i1 %88, i1 %291, i1 false
  br i1 %292, label %293, label %313

293:                                              ; preds = %288
  %294 = getelementptr inbounds i8, ptr %6, i64 5604
  %295 = load i32, ptr %294, align 4
  %296 = add i32 %295, %245
  %297 = zext i32 %245 to i64
  br label %298

298:                                              ; preds = %293, %304
  %299 = phi i64 [ 0, %293 ], [ %306, %304 ]
  %300 = phi i32 [ %295, %293 ], [ %305, %304 ]
  %301 = getelementptr inbounds [48 x i8], ptr %9, i64 0, i64 %299
  %302 = load i8, ptr %301, align 1, !tbaa !26
  %303 = icmp eq i8 %302, 48
  br i1 %303, label %304, label %308

304:                                              ; preds = %298
  %305 = add nsw i32 %300, 1
  %306 = add nuw nsw i64 %299, 1
  %307 = icmp eq i64 %306, %297
  br i1 %307, label %310, label %298
308:                                              ; preds = %298
  %309 = trunc i64 %299 to i32
  br label %310

310:                                              ; preds = %304, %308
  %311 = phi i32 [ %300, %308 ], [ %296, %304 ]
  %312 = phi i32 [ %309, %308 ], [ %286, %304 ]
  store i32 %311, ptr %294, align 4
  br label %313

313:                                              ; preds = %310, %288
  %314 = phi i32 [ 0, %288 ], [ %312, %310 ]
  %315 = sub nsw i32 %290, %314
  %316 = getelementptr inbounds i8, ptr %6, i64 5604
  %317 = load i32, ptr %316, align 4, !tbaa !35
  %318 = add nsw i32 %317, %315
  %319 = add nsw i32 %318, -1
  br i1 %88, label %320, label %324

320:                                              ; preds = %313
  %321 = icmp slt i32 %318, -5
  %322 = icmp sgt i32 %317, 6
  %323 = or i1 %322, %321
  br i1 %323, label %384, label %324

324:                                              ; preds = %320, %313
  %325 = icmp slt i32 %318, 1
  br i1 %325, label %326, label %346

326:                                              ; preds = %324
  %327 = zext i32 %99 to i64
  %328 = getelementptr inbounds i8, ptr %0, i64 %327
  store i8 48, ptr %328, align 1, !tbaa !26
  %329 = or i32 %99, 2
  %330 = getelementptr inbounds i8, ptr %328, i64 1
  store i8 46, ptr %330, align 1, !tbaa !26
  %331 = icmp slt i32 %318, 0
  br i1 %331, label %332, label %346

332:                                              ; preds = %326
  %333 = zext i32 %329 to i64
  %334 = sub i32 %314, %317
  %335 = add i32 %334, %243
  %336 = tail call i32 @llvm.smax.i32(i32 %335, i32 1)
  %337 = add nuw i32 %329, %336
  %338 = zext i32 %337 to i64
  br label %339

339:                                              ; preds = %332, %339
  %340 = phi i64 [ %333, %332 ], [ %341, %339 ]
  %341 = add nuw nsw i64 %340, 1
  %342 = getelementptr inbounds i8, ptr %0, i64 %340
  store i8 48, ptr %342, align 1, !tbaa !26
  %343 = icmp eq i64 %341, %338
  br i1 %343, label %344, label %339
344:                                              ; preds = %339
  %345 = trunc i64 %341 to i32
  br label %346

346:                                              ; preds = %344, %326, %324
  %347 = phi i32 [ %99, %324 ], [ %329, %326 ], [ %345, %344 ]
  %348 = icmp sgt i32 %314, %289
  br i1 %348, label %351, label %349

349:                                              ; preds = %346
  %350 = sext i32 %314 to i64
  br label %357

351:                                              ; preds = %374, %346
  %352 = phi i32 [ %347, %346 ], [ %375, %374 ]
  %353 = phi i32 [ %318, %346 ], [ %317, %374 ]
  %354 = icmp sgt i32 %353, 0
  br i1 %354, label %355, label %452

355:                                              ; preds = %351
  %356 = sext i32 %352 to i64
  br label %377

357:                                              ; preds = %349, %374
  %358 = phi i64 [ %246, %349 ], [ %376, %374 ]
  %359 = phi i32 [ %318, %349 ], [ %366, %374 ]
  %360 = phi i32 [ %347, %349 ], [ %375, %374 ]
  %361 = getelementptr inbounds [48 x i8], ptr %9, i64 0, i64 %358
  %362 = load i8, ptr %361, align 1, !tbaa !26
  %363 = add nsw i32 %360, 1
  %364 = sext i32 %360 to i64
  %365 = getelementptr inbounds i8, ptr %0, i64 %364
  store i8 %362, ptr %365, align 1, !tbaa !26
  %366 = add nsw i32 %359, -1
  %367 = icmp eq i32 %366, 0
  %368 = icmp sgt i64 %358, %350
  %369 = and i1 %367, %368
  br i1 %369, label %370, label %374

370:                                              ; preds = %357
  %371 = add nsw i32 %360, 2
  %372 = sext i32 %363 to i64
  %373 = getelementptr inbounds i8, ptr %0, i64 %372
  store i8 46, ptr %373, align 1, !tbaa !26
  br label %374

374:                                              ; preds = %357, %370
  %375 = phi i32 [ %371, %370 ], [ %363, %357 ]
  %376 = add nsw i64 %358, -1
  br i1 %368, label %357, label %351
377:                                              ; preds = %355, %377
  %378 = phi i64 [ %356, %355 ], [ %381, %377 ]
  %379 = phi i32 [ %353, %355 ], [ %380, %377 ]
  %380 = add nsw i32 %379, -1
  %381 = add nsw i64 %378, 1
  %382 = getelementptr inbounds i8, ptr %0, i64 %378
  store i8 48, ptr %382, align 1, !tbaa !26
  %383 = icmp sgt i32 %379, 1
  br i1 %383, label %377, label %450
384:                                              ; preds = %320
  %385 = add nuw nsw i32 %99, 1
  %386 = zext i32 %99 to i64
  %387 = getelementptr inbounds i8, ptr %0, i64 %386
  store i8 %255, ptr %387, align 1, !tbaa !26
  %388 = icmp sgt i32 %315, 1
  br i1 %388, label %389, label %393

389:                                              ; preds = %384
  %390 = or i32 %99, 2
  %391 = zext i32 %385 to i64
  %392 = getelementptr inbounds i8, ptr %0, i64 %391
  store i8 46, ptr %392, align 1, !tbaa !26
  br label %393

393:                                              ; preds = %389, %384
  %394 = phi i32 [ %390, %389 ], [ %385, %384 ]
  %395 = icmp slt i32 %314, %289
  br i1 %395, label %396, label %403

396:                                              ; preds = %393
  %397 = zext i32 %394 to i64
  %398 = sub i32 %394, %314
  %399 = add i32 %398, %245
  %400 = zext i32 %399 to i64
  br label %441

401:                                              ; preds = %441
  %402 = trunc i64 %447 to i32
  br label %403

403:                                              ; preds = %401, %393
  %404 = phi i32 [ %394, %393 ], [ %402, %401 ]
  %405 = sext i32 %404 to i64
  %406 = getelementptr inbounds i8, ptr %0, i64 %405
  store i8 101, ptr %406, align 1, !tbaa !26
  %407 = icmp slt i32 %318, 1
  %408 = select i1 %407, i8 45, i8 43
  %409 = getelementptr i8, ptr %406, i64 1
  store i8 %408, ptr %409, align 1, !tbaa !26
  %410 = sub nsw i32 1, %318
  %411 = select i1 %407, i32 %410, i32 %319
  call void @llvm.lifetime.start.p0(i64 12, ptr nonnull %4) #9
  br label %412

412:                                              ; preds = %412, %403
  %413 = phi i32 [ %423, %412 ], [ 1, %403 ]
  %414 = phi i64 [ %419, %412 ], [ 0, %403 ]
  %415 = phi i32 [ %421, %412 ], [ %411, %403 ]
  %416 = urem i32 %415, 10
  %417 = trunc i32 %416 to i8
  %418 = or i8 %417, 48
  %419 = add nuw nsw i64 %414, 1
  %420 = getelementptr inbounds [12 x i8], ptr %4, i64 0, i64 %414
  store i8 %418, ptr %420, align 1, !tbaa !26
  %421 = udiv i32 %415, 10
  %422 = icmp ult i32 %415, 10
  %423 = add nuw i32 %413, 1
  br i1 %422, label %424, label %412
424:                                              ; preds = %412
  %425 = add nsw i32 %404, 2
  %426 = sext i32 %425 to i64
  %427 = getelementptr inbounds i8, ptr %0, i64 %426
  %428 = and i64 %414, 4294967295
  %429 = zext i32 %413 to i64
  br label %430

430:                                              ; preds = %430, %424
  %431 = phi i64 [ 0, %424 ], [ %436, %430 ]
  %432 = sub nuw nsw i64 %428, %431
  %433 = getelementptr inbounds [12 x i8], ptr %4, i64 0, i64 %432
  %434 = load i8, ptr %433, align 1, !tbaa !26
  %435 = getelementptr inbounds i8, ptr %427, i64 %431
  store i8 %434, ptr %435, align 1, !tbaa !26
  %436 = add nuw nsw i64 %431, 1
  %437 = icmp eq i64 %436, %429
  br i1 %437, label %438, label %430
438:                                              ; preds = %430
  %439 = trunc i64 %419 to i32
  call void @llvm.lifetime.end.p0(i64 12, ptr nonnull %4) #9
  %440 = add nsw i32 %425, %439
  br label %452

441:                                              ; preds = %396, %441
  %442 = phi i64 [ %397, %396 ], [ %447, %441 ]
  %443 = phi i64 [ %246, %396 ], [ %444, %441 ]
  %444 = add nsw i64 %443, -1
  %445 = getelementptr inbounds [48 x i8], ptr %9, i64 0, i64 %444
  %446 = load i8, ptr %445, align 1, !tbaa !26
  %447 = add nuw nsw i64 %442, 1
  %448 = getelementptr inbounds i8, ptr %0, i64 %442
  store i8 %446, ptr %448, align 1, !tbaa !26
  %449 = icmp eq i64 %447, %400
  br i1 %449, label %401, label %441
450:                                              ; preds = %377
  %451 = trunc i64 %381 to i32
  br label %452

452:                                              ; preds = %450, %351, %438
  %453 = phi i32 [ %440, %438 ], [ %352, %351 ], [ %451, %450 ]
  call void @llvm.lifetime.end.p0(i64 48, ptr nonnull %9) #9
  br label %456

454:                                              ; preds = %105
  %455 = trunc i64 %110 to i32
  br label %456

456:                                              ; preds = %454, %452, %117
  %457 = phi i32 [ %453, %452 ], [ %118, %117 ], [ %455, %454 ]
  call void @llvm.lifetime.end.p0(i64 5616, ptr nonnull %6) #9
  call void @llvm.lifetime.end.p0(i64 36, ptr nonnull %5) #9
  ret i32 %457
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
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %5) #9
  %48 = icmp sgt i32 %47, -1
  %49 = select i1 %48, ptr %1, ptr %0
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %5, ptr noundef nonnull readonly align 4 dereferenceable(5604) %49, i64 5604, i1 false)
  %50 = tail call i32 @llvm.abs.i32(i32 %47, i1 true)
  call fastcc void @power(ptr noundef %5, i32 noundef %2, i32 noundef %50) #10
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
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %5) #9
  br i1 %90, label %91, label %94

91:                                               ; preds = %89
  %92 = add nsw i32 %47, -1
  br label %46
93:                                               ; preds = %71, %54, %77, %57
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %5) #9
  br label %94

94:                                               ; preds = %89, %93
  %95 = getelementptr inbounds i8, ptr %4, i64 4
  %96 = icmp eq i32 %6, 0
  br label %97

97:                                               ; preds = %94, %145
  %98 = phi i32 [ %99, %145 ], [ %47, %94 ]
  %99 = add nsw i32 %98, 1
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %4) #9
  %100 = icmp sgt i32 %98, -2
  %101 = select i1 %100, ptr %1, ptr %0
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %4, ptr noundef nonnull readonly align 4 dereferenceable(5604) %101, i64 5604, i1 false)
  %102 = tail call i32 @llvm.abs.i32(i32 %99, i1 true)
  call fastcc void @power(ptr noundef %4, i32 noundef %2, i32 noundef %102) #10
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
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %4) #9
  %147 = icmp sgt i32 %146, -1
  br i1 %147, label %97, label %148
148:                                              ; preds = %145
  ret i32 %98
}

; Function Attrs: nounwind memory(argmem: readwrite, inaccessiblemem: write)
define internal fastcc void @rounded(ptr noalias nonnull sret(%struct.tzrt_big) align 4 %0, ptr noundef nonnull readonly %1, ptr noundef nonnull readonly %2, i32 noundef %3, i32 noundef %4) unnamed_addr #2 {
  %6 = alloca %struct.tzrt_big, align 4
  %7 = alloca %struct.tzrt_big, align 4
  %8 = alloca %struct.tzrt_big, align 4
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %6) #9
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %6, ptr noundef nonnull align 4 dereferenceable(5604) %1, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %7) #9
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %7, ptr noundef nonnull align 4 dereferenceable(5604) %2, i64 5604, i1 false), !tbaa.struct !30
  %9 = icmp sgt i32 %4, -1
  br i1 %9, label %10, label %11

10:                                               ; preds = %5
  call fastcc void @power(ptr noundef %6, i32 noundef %3, i32 noundef %4) #10
  br label %13

11:                                               ; preds = %5
  %12 = sub nsw i32 0, %4
  call fastcc void @power(ptr noundef %7, i32 noundef %3, i32 noundef %12) #10
  br label %13

13:                                               ; preds = %11, %10
  call fastcc void @divide(ptr sret(%struct.tzrt_big) align 4 %0, ptr noundef %6, ptr noundef %7) #10
  %14 = load i32, ptr %6, align 4, !tbaa !34
  %15 = icmp sgt i32 %14, 0
  br i1 %15, label %16, label %19

16:                                               ; preds = %13
  %17 = getelementptr inbounds i8, ptr %6, i64 4
  %18 = zext i32 %14 to i64
  br label %22

19:                                               ; preds = %22, %13
  %20 = phi i64 [ 0, %13 ], [ %31, %22 ]
  %21 = icmp eq i64 %20, 0
  br i1 %21, label %43, label %34

22:                                               ; preds = %22, %16
  %23 = phi i64 [ 0, %16 ], [ %32, %22 ]
  %24 = phi i64 [ 0, %16 ], [ %31, %22 ]
  %25 = getelementptr inbounds [1400 x i32], ptr %17, i64 0, i64 %23
  %26 = load i32, ptr %25, align 4, !tbaa !24
  %27 = zext i32 %26 to i64
  %28 = shl nuw nsw i64 %27, 1
  %29 = add nuw nsw i64 %28, %24
  %30 = trunc i64 %29 to i32
  store i32 %30, ptr %25, align 4, !tbaa !24
  %31 = lshr i64 %29, 32
  %32 = add nuw nsw i64 %23, 1
  %33 = icmp eq i64 %32, %18
  br i1 %33, label %19, label %22
34:                                               ; preds = %19
  %35 = icmp eq i32 %14, 1400
  br i1 %35, label %36, label %37

36:                                               ; preds = %34
  tail call void @llvm.trap()
  unreachable

37:                                               ; preds = %34
  %38 = trunc i64 %20 to i32
  %39 = getelementptr inbounds i8, ptr %6, i64 4
  %40 = add nsw i32 %14, 1
  store i32 %40, ptr %6, align 4, !tbaa !34
  %41 = sext i32 %14 to i64
  %42 = getelementptr inbounds [1400 x i32], ptr %39, i64 0, i64 %41
  store i32 %38, ptr %42, align 4, !tbaa !24
  br label %43

43:                                               ; preds = %19, %37
  %44 = load i32, ptr %6, align 4, !tbaa !34
  %45 = load i32, ptr %7, align 4, !tbaa !34
  %46 = icmp eq i32 %44, %45
  br i1 %46, label %47, label %53

47:                                               ; preds = %43
  %48 = getelementptr inbounds i8, ptr %6, i64 4
  %49 = getelementptr inbounds i8, ptr %7, i64 4
  %50 = icmp eq i32 %44, 0
  br i1 %50, label %69, label %51

51:                                               ; preds = %47
  %52 = sext i32 %44 to i64
  br label %58

53:                                               ; preds = %43
  %54 = icmp slt i32 %44, %45
  %55 = select i1 %54, i32 -1, i32 1
  br label %69

56:                                               ; preds = %58
  %57 = icmp eq i64 %60, 0
  br i1 %57, label %69, label %58
58:                                               ; preds = %51, %56
  %59 = phi i64 [ %52, %51 ], [ %60, %56 ]
  %60 = add nsw i64 %59, -1
  %61 = getelementptr inbounds [1400 x i32], ptr %48, i64 0, i64 %60
  %62 = load i32, ptr %61, align 4, !tbaa !24
  %63 = getelementptr inbounds [1400 x i32], ptr %49, i64 0, i64 %60
  %64 = load i32, ptr %63, align 4, !tbaa !24
  %65 = icmp eq i32 %62, %64
  br i1 %65, label %56, label %66
66:                                               ; preds = %58
  %67 = icmp ult i32 %62, %64
  %68 = select i1 %67, i32 -1, i32 1
  br label %69

69:                                               ; preds = %56, %47, %53, %66
  %70 = phi i32 [ %55, %53 ], [ %68, %66 ], [ 0, %47 ], [ 0, %56 ]
  %71 = icmp sgt i32 %70, 0
  br i1 %71, label %82, label %72

72:                                               ; preds = %69
  %73 = icmp eq i32 %70, 0
  %74 = load i32, ptr %0, align 4
  %75 = icmp ne i32 %74, 0
  %76 = select i1 %73, i1 %75, i1 false
  br i1 %76, label %77, label %122

77:                                               ; preds = %72
  %78 = getelementptr inbounds i8, ptr %0, i64 4
  %79 = load i32, ptr %78, align 4, !tbaa !24
  %80 = and i32 %79, 1
  %81 = icmp eq i32 %80, 0
  br i1 %81, label %122, label %82

82:                                               ; preds = %77, %69
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %8) #9
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %8, i8 0, i64 5604, i1 false), !alias.scope !110
  %83 = getelementptr inbounds i8, ptr %8, i64 4
  store i32 1, ptr %8, align 4, !tbaa !34, !alias.scope !110
  store i32 1, ptr %83, align 4, !tbaa !24, !alias.scope !110
  %84 = load i32, ptr %0, align 4, !tbaa !34
  %85 = tail call i32 @llvm.smax.i32(i32 %84, i32 1)
  store i32 %85, ptr %0, align 4
  %86 = getelementptr inbounds i8, ptr %0, i64 4
  %87 = sext i32 %84 to i64
  %88 = zext i32 %85 to i64
  br label %91

89:                                               ; preds = %106
  %90 = icmp samesign ult i64 %109, 4294967296
  br i1 %90, label %121, label %115

91:                                               ; preds = %106, %82
  %92 = phi i64 [ 0, %82 ], [ %113, %106 ]
  %93 = phi i64 [ 0, %82 ], [ %112, %106 ]
  %94 = icmp slt i64 %92, %87
  br i1 %94, label %95, label %99

95:                                               ; preds = %91
  %96 = getelementptr inbounds [1400 x i32], ptr %86, i64 0, i64 %92
  %97 = load i32, ptr %96, align 4, !tbaa !24
  %98 = zext i32 %97 to i64
  br label %99

99:                                               ; preds = %95, %91
  %100 = phi i64 [ %98, %95 ], [ 0, %91 ]
  %101 = icmp eq i64 %92, 0
  br i1 %101, label %102, label %106

102:                                              ; preds = %99
  %103 = getelementptr inbounds [1400 x i32], ptr %83, i64 0, i64 %92
  %104 = load i32, ptr %103, align 4, !tbaa !24
  %105 = zext i32 %104 to i64
  br label %106

106:                                              ; preds = %102, %99
  %107 = phi i64 [ %105, %102 ], [ 0, %99 ]
  %108 = add nuw nsw i64 %100, %93
  %109 = add nuw nsw i64 %108, %107
  %110 = trunc i64 %109 to i32
  %111 = getelementptr inbounds [1400 x i32], ptr %86, i64 0, i64 %92
  store i32 %110, ptr %111, align 4, !tbaa !24
  %112 = lshr i64 %109, 32
  %113 = add nuw nsw i64 %92, 1
  %114 = icmp eq i64 %113, %88
  br i1 %114, label %89, label %91
115:                                              ; preds = %89
  %116 = icmp eq i32 %84, 1400
  br i1 %116, label %117, label %118

117:                                              ; preds = %115
  tail call void @llvm.trap()
  unreachable

118:                                              ; preds = %115
  %119 = add nuw nsw i32 %85, 1
  store i32 %119, ptr %0, align 4, !tbaa !34
  %120 = getelementptr inbounds [1400 x i32], ptr %86, i64 0, i64 %88
  store i32 1, ptr %120, align 4, !tbaa !24
  br label %121

121:                                              ; preds = %89, %118
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %8) #9
  br label %122

122:                                              ; preds = %121, %77, %72
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %7) #9
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %6) #9
  ret void
}

; Function Attrs: mustprogress nocallback nofree nounwind willreturn memory(argmem: write)
declare void @llvm.memset.p0.i64(ptr writeonly, i8, i64, i1 immarg) #4

; Function Attrs: cold noreturn nounwind memory(inaccessiblemem: write)
declare void @llvm.trap() #5

; Function Attrs: mustprogress nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i32 @llvm.ctlz.i32(i32, i1 immarg) #6

; Function Attrs: nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i32 @llvm.smin.i32(i32, i32) #7

; Function Attrs: nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i32 @llvm.scmp.i32.i32(i32, i32) #7

; Function Attrs: nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i32 @llvm.abs.i32(i32, i1 immarg) #7

; Function Attrs: nocallback nofree nosync nounwind willreturn memory(inaccessiblemem: readwrite)
declare void @llvm.experimental.noalias.scope.decl(metadata) #8

; Function Attrs: nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i32 @llvm.smax.i32(i32, i32) #7

attributes #0 = { nounwind memory(argmem: readwrite, inaccessiblemem: readwrite) "min-legal-vector-width"="0" "no-builtins" "no-trapping-math"="true" "stack-protector-buffer-size"="8" "tune-cpu"="generic" }
attributes #1 = { mustprogress nocallback nofree nosync nounwind willreturn memory(argmem: readwrite) }
attributes #2 = { nounwind memory(argmem: readwrite, inaccessiblemem: write) "min-legal-vector-width"="0" "no-builtins" "no-trapping-math"="true" "stack-protector-buffer-size"="8" "tune-cpu"="generic" }
attributes #3 = { mustprogress nocallback nofree nounwind willreturn memory(argmem: readwrite) }
attributes #4 = { mustprogress nocallback nofree nounwind willreturn memory(argmem: write) }
attributes #5 = { cold noreturn nounwind memory(inaccessiblemem: write) }
attributes #6 = { mustprogress nocallback nofree nosync nounwind speculatable willreturn memory(none) }
attributes #7 = { nocallback nofree nosync nounwind speculatable willreturn memory(none) }
attributes #8 = { nocallback nofree nosync nounwind willreturn memory(inaccessiblemem: readwrite) }
attributes #9 = { nounwind }
attributes #10 = { nobuiltin "no-builtins" }


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
!66 = distinct !{!66, !67, !"small: argument 0"}
!67 = distinct !{!67, !"small"}
!68 = distinct !{!68, !28, !29}
!69 = distinct !{!69, !28, !29}
!70 = !{!71}
!71 = distinct !{!71, !72, !"format: argument 0"}
!72 = distinct !{!72, !"format"}
!73 = !{!74}
!74 = distinct !{!74, !75, !"format: argument 0"}
!75 = distinct !{!75, !"format"}
!76 = !{!77}
!77 = distinct !{!77, !78, !"format: argument 0"}
!78 = distinct !{!78, !"format"}
!79 = !{!80}
!80 = distinct !{!80, !81, !"small: argument 0"}
!81 = distinct !{!81, !"small"}
!82 = !{!83}
!83 = distinct !{!83, !84, !"small: argument 0"}
!84 = distinct !{!84, !"small"}
!85 = !{!86}
!86 = distinct !{!86, !87, !"multiply: argument 0"}
!87 = distinct !{!87, !"multiply"}
!88 = !{!89}
!89 = distinct !{!89, !90, !"small: argument 0"}
!90 = distinct !{!90, !"small"}
!91 = distinct !{!91, !28, !29}
!92 = distinct !{!92, !28, !29}
!93 = !{!94}
!94 = distinct !{!94, !95, !"format: argument 0"}
!95 = distinct !{!95, !"format"}
!96 = distinct !{!96, !28, !29}
!97 = !{!98}
!98 = distinct !{!98, !99, !"small: argument 0"}
!99 = distinct !{!99, !"small"}
!100 = distinct !{!100, !28, !29}
!101 = distinct !{!101, !28, !29}
!102 = distinct !{!102, !28, !29}
!103 = distinct !{!103, !28, !29}
!104 = distinct !{!104, !28, !29}
!105 = distinct !{!105, !28, !29}
!106 = distinct !{!106, !28, !29}
!107 = distinct !{!107, !28, !29}
!108 = distinct !{!108, !28, !29}
!109 = distinct !{!109, !28, !29}
!110 = !{!111}
!111 = distinct !{!111, !112, !"small: argument 0"}
!112 = distinct !{!112, !"small"}
