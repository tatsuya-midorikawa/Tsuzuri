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
  call void @llvm.lifetime.start.p0(i64 36, ptr nonnull %6) #14
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
  call void @llvm.lifetime.start.p0(i64 5616, ptr nonnull %7) #14
  call fastcc void @decode(ptr sret(%struct.tzrt_number) align 4 %7, ptr noundef %1, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %6) #15
  call void @llvm.lifetime.start.p0(i64 5616, ptr nonnull %8) #14
  call fastcc void @decode(ptr sret(%struct.tzrt_number) align 4 %8, ptr noundef %2, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %6) #15
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
  br i1 %161, label %221, label %162

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

195:                                              ; preds = %167, %176, %184
  %196 = phi i64 [ %181, %176 ], [ %173, %167 ], [ %192, %184 ]
  %197 = phi i64 [ %183, %176 ], [ %175, %167 ], [ %194, %184 ]
  %198 = zext i64 %197 to i128
  %199 = shl nuw i128 %198, 64
  %200 = zext i64 %196 to i128
  %201 = or i128 %199, %200
  switch i32 %89, label %202 [
    i32 8, label %205
    i32 16, label %207
    i32 32, label %209
    i32 64, label %211
    i32 128, label %212
  ]

202:                                              ; preds = %195
  %203 = lshr exact i32 %89, 3
  %204 = zext i32 %203 to i64
  br label %213

205:                                              ; preds = %195
  %206 = trunc i64 %196 to i8
  store i8 %206, ptr %0, align 1, !tbaa !26
  br label %498

207:                                              ; preds = %195
  %208 = trunc i64 %196 to i16
  store i16 %208, ptr %0, align 1
  br label %498

209:                                              ; preds = %195
  %210 = trunc i64 %196 to i32
  store i32 %210, ptr %0, align 1
  br label %498

211:                                              ; preds = %195
  store i64 %196, ptr %0, align 1
  br label %498

212:                                              ; preds = %195
  store i128 %201, ptr %0, align 1
  br label %498

213:                                              ; preds = %213, %202
  %214 = phi i64 [ 0, %202 ], [ %219, %213 ]
  %215 = phi i128 [ %201, %202 ], [ %218, %213 ]
  %216 = trunc i128 %215 to i8
  %217 = getelementptr inbounds i8, ptr %0, i64 %214
  store i8 %216, ptr %217, align 1, !tbaa !26
  %218 = lshr i128 %215, 8
  %219 = add nuw nsw i64 %214, 1
  %220 = icmp eq i64 %219, %204
  br i1 %220, label %498, label %213
221:                                              ; preds = %158
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %9) #14
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %9, ptr noundef nonnull align 4 dereferenceable(5604) %7, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %10) #14
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %10, i8 0, i64 5604, i1 false), !alias.scope !31
  %222 = getelementptr inbounds i8, ptr %10, i64 4
  store i32 1, ptr %10, align 4, !tbaa !34, !alias.scope !31
  store i32 1, ptr %222, align 4, !tbaa !24, !alias.scope !31
  br i1 %100, label %223, label %392

223:                                              ; preds = %221
  %224 = getelementptr inbounds i8, ptr %7, i64 5604
  %225 = load i32, ptr %224, align 4, !tbaa !35
  %226 = getelementptr inbounds i8, ptr %8, i64 5604
  %227 = load i32, ptr %226, align 4, !tbaa !35
  %228 = tail call i32 @llvm.smin.i32(i32 %225, i32 %227)
  %229 = sub nsw i32 %225, %228
  call fastcc void @power(ptr noundef %9, i32 noundef %93, i32 noundef %229) #15
  %230 = sub nsw i32 %227, %228
  call fastcc void @power(ptr noundef %8, i32 noundef %93, i32 noundef %230) #15
  %231 = load i32, ptr %103, align 4, !tbaa !20
  %232 = icmp eq i32 %102, %231
  %233 = load i32, ptr %9, align 4, !tbaa !34
  %234 = load i32, ptr %8, align 4, !tbaa !34
  br i1 %232, label %235, label %276

235:                                              ; preds = %223
  %236 = tail call i32 @llvm.smax.i32(i32 %233, i32 %234)
  store i32 %236, ptr %9, align 4
  %237 = icmp sgt i32 %236, 0
  br i1 %237, label %238, label %468

238:                                              ; preds = %235
  %239 = getelementptr inbounds i8, ptr %9, i64 4
  %240 = getelementptr inbounds i8, ptr %8, i64 4
  %241 = sext i32 %233 to i64
  %242 = zext i32 %236 to i64
  %243 = sext i32 %234 to i64
  br label %246

244:                                              ; preds = %261
  %245 = icmp samesign ult i64 %264, 4294967296
  br i1 %245, label %468, label %270

246:                                              ; preds = %261, %238
  %247 = phi i64 [ 0, %238 ], [ %268, %261 ]
  %248 = phi i64 [ 0, %238 ], [ %267, %261 ]
  %249 = icmp slt i64 %247, %241
  br i1 %249, label %250, label %254

250:                                              ; preds = %246
  %251 = getelementptr inbounds [1400 x i32], ptr %239, i64 0, i64 %247
  %252 = load i32, ptr %251, align 4, !tbaa !24
  %253 = zext i32 %252 to i64
  br label %254

254:                                              ; preds = %250, %246
  %255 = phi i64 [ %253, %250 ], [ 0, %246 ]
  %256 = icmp slt i64 %247, %243
  br i1 %256, label %257, label %261

257:                                              ; preds = %254
  %258 = getelementptr inbounds [1400 x i32], ptr %240, i64 0, i64 %247
  %259 = load i32, ptr %258, align 4, !tbaa !24
  %260 = zext i32 %259 to i64
  br label %261

261:                                              ; preds = %257, %254
  %262 = phi i64 [ %260, %257 ], [ 0, %254 ]
  %263 = add nuw nsw i64 %255, %248
  %264 = add nuw nsw i64 %263, %262
  %265 = trunc i64 %264 to i32
  %266 = getelementptr inbounds [1400 x i32], ptr %239, i64 0, i64 %247
  store i32 %265, ptr %266, align 4, !tbaa !24
  %267 = lshr i64 %264, 32
  %268 = add nuw nsw i64 %247, 1
  %269 = icmp eq i64 %268, %242
  br i1 %269, label %244, label %246
270:                                              ; preds = %244
  %271 = icmp eq i32 %236, 1400
  br i1 %271, label %272, label %273

272:                                              ; preds = %270
  tail call void @llvm.trap()
  unreachable

273:                                              ; preds = %270
  %274 = add nuw nsw i32 %236, 1
  store i32 %274, ptr %9, align 4, !tbaa !34
  %275 = getelementptr inbounds [1400 x i32], ptr %239, i64 0, i64 %242
  store i32 1, ptr %275, align 4, !tbaa !24
  br label %468

276:                                              ; preds = %223
  %277 = icmp eq i32 %233, %234
  br i1 %277, label %278, label %284

278:                                              ; preds = %276
  %279 = getelementptr inbounds i8, ptr %9, i64 4
  %280 = getelementptr inbounds i8, ptr %8, i64 4
  %281 = icmp eq i32 %233, 0
  br i1 %281, label %300, label %282

282:                                              ; preds = %278
  %283 = sext i32 %233 to i64
  br label %289

284:                                              ; preds = %276
  %285 = icmp slt i32 %233, %234
  %286 = select i1 %285, i32 -1, i32 1
  br label %300

287:                                              ; preds = %289
  %288 = icmp eq i64 %291, 0
  br i1 %288, label %300, label %289
289:                                              ; preds = %282, %287
  %290 = phi i64 [ %283, %282 ], [ %291, %287 ]
  %291 = add nsw i64 %290, -1
  %292 = getelementptr inbounds [1400 x i32], ptr %279, i64 0, i64 %291
  %293 = load i32, ptr %292, align 4, !tbaa !24
  %294 = getelementptr inbounds [1400 x i32], ptr %280, i64 0, i64 %291
  %295 = load i32, ptr %294, align 4, !tbaa !24
  %296 = icmp eq i32 %293, %295
  br i1 %296, label %287, label %297
297:                                              ; preds = %289
  %298 = icmp ult i32 %293, %295
  %299 = select i1 %298, i32 -1, i32 1
  br label %300

300:                                              ; preds = %287, %278, %284, %297
  %301 = phi i32 [ %286, %284 ], [ %299, %297 ], [ 0, %278 ], [ 0, %287 ]
  %302 = icmp sgt i32 %301, -1
  br i1 %302, label %303, label %344

303:                                              ; preds = %300
  %304 = icmp sgt i32 %233, 0
  br i1 %304, label %305, label %310

305:                                              ; preds = %303
  %306 = getelementptr inbounds i8, ptr %8, i64 4
  %307 = getelementptr inbounds i8, ptr %9, i64 4
  %308 = zext i32 %233 to i64
  %309 = sext i32 %234 to i64
  br label %324

310:                                              ; preds = %332, %303
  %311 = getelementptr inbounds i8, ptr %9, i64 4
  %312 = icmp eq i32 %233, 0
  br i1 %312, label %387, label %313

313:                                              ; preds = %310
  %314 = sext i32 %233 to i64
  br label %315

315:                                              ; preds = %321, %313
  %316 = phi i64 [ %314, %313 ], [ %317, %321 ]
  %317 = add nsw i64 %316, -1
  %318 = getelementptr inbounds [1400 x i32], ptr %311, i64 0, i64 %317
  %319 = load i32, ptr %318, align 4, !tbaa !24
  %320 = icmp eq i32 %319, 0
  br i1 %320, label %321, label %387

321:                                              ; preds = %315
  %322 = trunc i64 %317 to i32
  store i32 %322, ptr %9, align 4, !tbaa !34
  %323 = icmp eq i64 %317, 0
  br i1 %323, label %387, label %315
324:                                              ; preds = %332, %305
  %325 = phi i64 [ 0, %305 ], [ %342, %332 ]
  %326 = phi i64 [ 0, %305 ], [ %341, %332 ]
  %327 = icmp slt i64 %325, %309
  br i1 %327, label %328, label %332

328:                                              ; preds = %324
  %329 = getelementptr inbounds [1400 x i32], ptr %306, i64 0, i64 %325
  %330 = load i32, ptr %329, align 4, !tbaa !24
  %331 = zext i32 %330 to i64
  br label %332

332:                                              ; preds = %328, %324
  %333 = phi i64 [ %331, %328 ], [ 0, %324 ]
  %334 = add nuw nsw i64 %333, %326
  %335 = getelementptr inbounds [1400 x i32], ptr %307, i64 0, i64 %325
  %336 = load i32, ptr %335, align 4, !tbaa !24
  %337 = zext i32 %336 to i64
  %338 = trunc i64 %334 to i32
  %339 = sub i32 %336, %338
  store i32 %339, ptr %335, align 4, !tbaa !24
  %340 = icmp samesign ugt i64 %334, %337
  %341 = zext i1 %340 to i64
  %342 = add nuw nsw i64 %325, 1
  %343 = icmp eq i64 %342, %308
  br i1 %343, label %310, label %324
344:                                              ; preds = %300
  %345 = icmp sgt i32 %234, 0
  br i1 %345, label %346, label %351

346:                                              ; preds = %344
  %347 = getelementptr inbounds i8, ptr %9, i64 4
  %348 = getelementptr inbounds i8, ptr %8, i64 4
  %349 = zext i32 %234 to i64
  %350 = sext i32 %233 to i64
  br label %365

351:                                              ; preds = %373, %344
  %352 = getelementptr inbounds i8, ptr %8, i64 4
  %353 = icmp eq i32 %234, 0
  br i1 %353, label %385, label %354

354:                                              ; preds = %351
  %355 = sext i32 %234 to i64
  br label %356

356:                                              ; preds = %362, %354
  %357 = phi i64 [ %355, %354 ], [ %358, %362 ]
  %358 = add nsw i64 %357, -1
  %359 = getelementptr inbounds [1400 x i32], ptr %352, i64 0, i64 %358
  %360 = load i32, ptr %359, align 4, !tbaa !24
  %361 = icmp eq i32 %360, 0
  br i1 %361, label %362, label %385

362:                                              ; preds = %356
  %363 = trunc i64 %358 to i32
  store i32 %363, ptr %8, align 4, !tbaa !34
  %364 = icmp eq i64 %358, 0
  br i1 %364, label %385, label %356
365:                                              ; preds = %373, %346
  %366 = phi i64 [ 0, %346 ], [ %383, %373 ]
  %367 = phi i64 [ 0, %346 ], [ %382, %373 ]
  %368 = icmp slt i64 %366, %350
  br i1 %368, label %369, label %373

369:                                              ; preds = %365
  %370 = getelementptr inbounds [1400 x i32], ptr %347, i64 0, i64 %366
  %371 = load i32, ptr %370, align 4, !tbaa !24
  %372 = zext i32 %371 to i64
  br label %373

373:                                              ; preds = %369, %365
  %374 = phi i64 [ %372, %369 ], [ 0, %365 ]
  %375 = add nuw nsw i64 %374, %367
  %376 = getelementptr inbounds [1400 x i32], ptr %348, i64 0, i64 %366
  %377 = load i32, ptr %376, align 4, !tbaa !24
  %378 = zext i32 %377 to i64
  %379 = trunc i64 %375 to i32
  %380 = sub i32 %377, %379
  store i32 %380, ptr %376, align 4, !tbaa !24
  %381 = icmp samesign ugt i64 %375, %378
  %382 = zext i1 %381 to i64
  %383 = add nuw nsw i64 %366, 1
  %384 = icmp eq i64 %383, %349
  br i1 %384, label %351, label %365
385:                                              ; preds = %356, %362, %351
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %9, ptr noundef nonnull align 4 dereferenceable(5604) %8, i64 5604, i1 false), !tbaa.struct !30
  %386 = load i32, ptr %103, align 4, !tbaa !20
  br label %387

387:                                              ; preds = %321, %315, %310, %385
  %388 = phi i32 [ %386, %385 ], [ %160, %310 ], [ %160, %315 ], [ %160, %321 ]
  %389 = load i32, ptr %9, align 4, !tbaa !34
  %390 = icmp eq i32 %389, 0
  %391 = select i1 %390, i32 0, i32 %388
  br label %468

392:                                              ; preds = %221
  %393 = icmp eq i32 %4, 2
  br i1 %393, label %394, label %462

394:                                              ; preds = %392
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %11) #14
  tail call void @llvm.experimental.noalias.scope.decl(metadata !40)
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %11, i8 0, i64 5604, i1 false), !alias.scope !40
  %395 = load i32, ptr %7, align 4, !tbaa !34, !noalias !40
  %396 = load i32, ptr %8, align 4, !tbaa !34, !noalias !40
  %397 = add nsw i32 %396, %395
  %398 = icmp sgt i32 %397, 1400
  br i1 %398, label %399, label %400

399:                                              ; preds = %394
  tail call void @llvm.trap()
  unreachable

400:                                              ; preds = %394
  store i32 %397, ptr %11, align 4, !tbaa !34, !alias.scope !40
  %401 = icmp sgt i32 %395, 0
  br i1 %401, label %402, label %416

402:                                              ; preds = %400
  %403 = icmp sgt i32 %396, 0
  %404 = getelementptr inbounds i8, ptr %7, i64 4
  %405 = getelementptr inbounds i8, ptr %8, i64 4
  %406 = getelementptr inbounds i8, ptr %11, i64 4
  %407 = sext i32 %396 to i64
  %408 = zext i32 %395 to i64
  %409 = zext i32 %396 to i64
  br label %410

410:                                              ; preds = %433, %402
  %411 = phi i64 [ 0, %402 ], [ %437, %433 ]
  br i1 %403, label %412, label %433

412:                                              ; preds = %410
  %413 = getelementptr inbounds [1400 x i32], ptr %404, i64 0, i64 %411
  %414 = load i32, ptr %413, align 4, !tbaa !24, !noalias !40
  %415 = zext i32 %414 to i64
  br label %439

416:                                              ; preds = %433, %400
  %417 = getelementptr inbounds i8, ptr %11, i64 4
  %418 = load i32, ptr %11, align 4, !tbaa !34, !alias.scope !40
  %419 = icmp eq i32 %418, 0
  br i1 %419, label %456, label %420

420:                                              ; preds = %416
  %421 = sext i32 %418 to i64
  br label %422

422:                                              ; preds = %428, %420
  %423 = phi i64 [ %421, %420 ], [ %424, %428 ]
  %424 = add nsw i64 %423, -1
  %425 = getelementptr inbounds [1400 x i32], ptr %417, i64 0, i64 %424
  %426 = load i32, ptr %425, align 4, !tbaa !24, !alias.scope !40
  %427 = icmp eq i32 %426, 0
  br i1 %427, label %428, label %456

428:                                              ; preds = %422
  %429 = trunc i64 %424 to i32
  store i32 %429, ptr %11, align 4, !tbaa !34, !alias.scope !40
  %430 = icmp eq i64 %424, 0
  br i1 %430, label %456, label %422
431:                                              ; preds = %439
  %432 = trunc i64 %453 to i32
  br label %433

433:                                              ; preds = %431, %410
  %434 = phi i32 [ 0, %410 ], [ %432, %431 ]
  %435 = add nsw i64 %411, %407
  %436 = getelementptr inbounds [1400 x i32], ptr %406, i64 0, i64 %435
  store i32 %434, ptr %436, align 4, !tbaa !24, !alias.scope !40
  %437 = add nuw nsw i64 %411, 1
  %438 = icmp eq i64 %437, %408
  br i1 %438, label %416, label %410
439:                                              ; preds = %439, %412
  %440 = phi i64 [ 0, %412 ], [ %454, %439 ]
  %441 = phi i64 [ 0, %412 ], [ %453, %439 ]
  %442 = getelementptr inbounds [1400 x i32], ptr %405, i64 0, i64 %440
  %443 = load i32, ptr %442, align 4, !tbaa !24, !noalias !40
  %444 = zext i32 %443 to i64
  %445 = mul nuw i64 %444, %415
  %446 = add nuw nsw i64 %440, %411
  %447 = getelementptr inbounds [1400 x i32], ptr %406, i64 0, i64 %446
  %448 = load i32, ptr %447, align 4, !tbaa !24, !alias.scope !40
  %449 = zext i32 %448 to i64
  %450 = add nuw nsw i64 %441, %449
  %451 = add nuw i64 %450, %445
  %452 = trunc i64 %451 to i32
  store i32 %452, ptr %447, align 4, !tbaa !24, !alias.scope !40
  %453 = lshr i64 %451, 32
  %454 = add nuw nsw i64 %440, 1
  %455 = icmp eq i64 %454, %409
  br i1 %455, label %431, label %439
456:                                              ; preds = %422, %428, %416
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %9, ptr noundef nonnull align 4 dereferenceable(5604) %11, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %11) #14
  %457 = getelementptr inbounds i8, ptr %7, i64 5604
  %458 = load i32, ptr %457, align 4, !tbaa !35
  %459 = getelementptr inbounds i8, ptr %8, i64 5604
  %460 = load i32, ptr %459, align 4, !tbaa !35
  %461 = add nsw i32 %460, %458
  br label %468

462:                                              ; preds = %392
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %10, ptr noundef nonnull align 4 dereferenceable(5604) %8, i64 5604, i1 false), !tbaa.struct !30
  %463 = getelementptr inbounds i8, ptr %7, i64 5604
  %464 = load i32, ptr %463, align 4, !tbaa !35
  %465 = getelementptr inbounds i8, ptr %8, i64 5604
  %466 = load i32, ptr %465, align 4, !tbaa !35
  %467 = sub nsw i32 %464, %466
  br label %468

468:                                              ; preds = %273, %244, %235, %456, %462, %387
  %469 = phi i32 [ %228, %387 ], [ %461, %456 ], [ %467, %462 ], [ %228, %235 ], [ %228, %244 ], [ %228, %273 ]
  %470 = phi i32 [ %391, %387 ], [ %160, %456 ], [ %160, %462 ], [ %160, %235 ], [ %160, %244 ], [ %160, %273 ]
  %471 = call fastcc { i64, i64 } @pack(ptr noundef %9, ptr noundef %10, i32 noundef %469, i32 noundef %470, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %6) #15
  %472 = extractvalue { i64, i64 } %471, 0
  %473 = extractvalue { i64, i64 } %471, 1
  %474 = zext i64 %473 to i128
  %475 = shl nuw i128 %474, 64
  %476 = zext i64 %472 to i128
  %477 = or i128 %475, %476
  switch i32 %89, label %478 [
    i32 8, label %481
    i32 16, label %483
    i32 32, label %485
    i32 64, label %487
    i32 128, label %488
  ]

478:                                              ; preds = %468
  %479 = lshr exact i32 %89, 3
  %480 = zext i32 %479 to i64
  br label %489

481:                                              ; preds = %468
  %482 = trunc i64 %472 to i8
  store i8 %482, ptr %0, align 1, !tbaa !26
  br label %497

483:                                              ; preds = %468
  %484 = trunc i64 %472 to i16
  store i16 %484, ptr %0, align 1
  br label %497

485:                                              ; preds = %468
  %486 = trunc i64 %472 to i32
  store i32 %486, ptr %0, align 1
  br label %497

487:                                              ; preds = %468
  store i64 %472, ptr %0, align 1
  br label %497

488:                                              ; preds = %468
  store i128 %477, ptr %0, align 1
  br label %497

489:                                              ; preds = %489, %478
  %490 = phi i64 [ 0, %478 ], [ %495, %489 ]
  %491 = phi i128 [ %477, %478 ], [ %494, %489 ]
  %492 = trunc i128 %491 to i8
  %493 = getelementptr inbounds i8, ptr %0, i64 %490
  store i8 %492, ptr %493, align 1, !tbaa !26
  %494 = lshr i128 %491, 8
  %495 = add nuw nsw i64 %490, 1
  %496 = icmp eq i64 %495, %480
  br i1 %496, label %497, label %489
497:                                              ; preds = %489, %481, %483, %485, %487, %488
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %10) #14
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %9) #14
  br label %498

498:                                              ; preds = %213, %212, %211, %209, %207, %205, %497
  call void @llvm.lifetime.end.p0(i64 5616, ptr nonnull %8) #14
  call void @llvm.lifetime.end.p0(i64 5616, ptr nonnull %7) #14
  call void @llvm.lifetime.end.p0(i64 36, ptr nonnull %6) #14
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
  switch i32 %9, label %26 [
    i32 8, label %10
    i32 16, label %13
    i32 32, label %16
    i32 64, label %19
    i32 128, label %21
  ]

10:                                               ; preds = %3
  %11 = load i8, ptr %1, align 1, !tbaa !26
  %12 = zext i8 %11 to i64
  br label %46

13:                                               ; preds = %3
  %14 = load i16, ptr %1, align 1
  %15 = zext i16 %14 to i64
  br label %46

16:                                               ; preds = %3
  %17 = load i32, ptr %1, align 1
  %18 = zext i32 %17 to i64
  br label %46

19:                                               ; preds = %3
  %20 = load i64, ptr %1, align 1
  br label %46

21:                                               ; preds = %3
  %22 = load i128, ptr %1, align 1
  %23 = trunc i128 %22 to i64
  %24 = lshr i128 %22, 64
  %25 = trunc i128 %24 to i64
  br label %46

26:                                               ; preds = %3
  %27 = add i32 %9, 7
  %28 = icmp ult i32 %27, 15
  br i1 %28, label %46, label %29

29:                                               ; preds = %26
  %30 = sdiv i32 %9, 8
  %31 = sext i32 %30 to i64
  br label %36

32:                                               ; preds = %36
  %33 = lshr i128 %40, 64
  %34 = trunc i128 %33 to i64
  %35 = trunc i128 %44 to i64
  br label %46

36:                                               ; preds = %36, %29
  %37 = phi i64 [ %31, %29 ], [ %39, %36 ]
  %38 = phi i128 [ 0, %29 ], [ %44, %36 ]
  %39 = add nsw i64 %37, -1
  %40 = shl i128 %38, 8
  %41 = getelementptr inbounds i8, ptr %1, i64 %39
  %42 = load i8, ptr %41, align 1, !tbaa !26
  %43 = zext i8 %42 to i128
  %44 = or i128 %40, %43
  %45 = icmp eq i64 %39, 0
  br i1 %45, label %32, label %36
46:                                               ; preds = %10, %13, %16, %19, %21, %26, %32
  %47 = phi i64 [ %12, %10 ], [ %15, %13 ], [ %18, %16 ], [ %20, %19 ], [ %23, %21 ], [ 0, %26 ], [ %35, %32 ]
  %48 = phi i64 [ 0, %10 ], [ 0, %13 ], [ 0, %16 ], [ 0, %19 ], [ %25, %21 ], [ 0, %26 ], [ %34, %32 ]
  %49 = zext i64 %48 to i128
  %50 = shl nuw i128 %49, 64
  %51 = zext i64 %47 to i128
  %52 = or i128 %50, %51
  tail call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5616) %0, i8 0, i64 5616, i1 false)
  %53 = add nsw i32 %9, -1
  %54 = zext i32 %53 to i128
  %55 = lshr i128 %52, %54
  %56 = trunc i128 %55 to i32
  %57 = getelementptr inbounds i8, ptr %0, i64 5608
  store i32 %56, ptr %57, align 4, !tbaa !20
  %58 = getelementptr inbounds i8, ptr %2, i64 28
  %59 = load i32, ptr %58, align 4, !tbaa !18
  %60 = icmp eq i32 %59, 0
  br i1 %60, label %97, label %61

61:                                               ; preds = %46
  %62 = getelementptr inbounds i8, ptr %2, i64 32
  %63 = load i32, ptr %62, align 8, !tbaa !19
  %64 = and i32 %63, %56
  store i32 %64, ptr %57, align 4, !tbaa !20
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %4) #14
  %65 = icmp eq i32 %64, 0
  br i1 %65, label %77, label %66

66:                                               ; preds = %61
  %67 = sub i128 0, %52
  %68 = icmp eq i32 %9, 128
  %69 = zext i32 %9 to i128
  %70 = shl nsw i128 -1, %69
  %71 = xor i128 %70, -1
  %72 = select i1 %68, i128 -1, i128 %71
  %73 = and i128 %72, %67
  %74 = trunc i128 %73 to i64
  %75 = lshr i128 %73, 64
  %76 = trunc i128 %75 to i64
  br label %77

77:                                               ; preds = %61, %66
  %78 = phi i64 [ %74, %66 ], [ %47, %61 ]
  %79 = phi i64 [ %76, %66 ], [ %48, %61 ]
  %80 = zext i64 %79 to i128
  %81 = shl nuw i128 %80, 64
  %82 = zext i64 %78 to i128
  %83 = or i128 %81, %82
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %4, i8 0, i64 5604, i1 false), !alias.scope !46
  %84 = icmp eq i128 %83, 0
  br i1 %84, label %96, label %85

85:                                               ; preds = %77
  %86 = getelementptr inbounds i8, ptr %4, i64 4
  br label %87

87:                                               ; preds = %87, %85
  %88 = phi i128 [ %83, %85 ], [ %94, %87 ]
  %89 = trunc i128 %88 to i32
  %90 = load i32, ptr %4, align 4, !tbaa !34, !alias.scope !46
  %91 = add nsw i32 %90, 1
  store i32 %91, ptr %4, align 4, !tbaa !34, !alias.scope !46
  %92 = sext i32 %90 to i64
  %93 = getelementptr inbounds [1400 x i32], ptr %86, i64 0, i64 %92
  store i32 %89, ptr %93, align 4, !tbaa !24, !alias.scope !46
  %94 = lshr i128 %88, 32
  %95 = icmp ult i128 %88, 4294967296
  br i1 %95, label %96, label %87
96:                                               ; preds = %87, %77
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %0, ptr noundef nonnull align 4 dereferenceable(5604) %4, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %4) #14
  br label %337

97:                                               ; preds = %46
  %98 = icmp eq i32 %53, 128
  %99 = shl nsw i128 -1, %54
  %100 = xor i128 %99, -1
  %101 = select i1 %98, i128 -1, i128 %100
  %102 = and i128 %52, %101
  %103 = load i32, ptr %2, align 8, !tbaa !4
  %104 = icmp eq i32 %103, 2
  br i1 %104, label %105, label %149

105:                                              ; preds = %97
  %106 = getelementptr inbounds i8, ptr %2, i64 16
  %107 = load i32, ptr %106, align 8, !tbaa !15
  %108 = zext i32 %107 to i128
  %109 = lshr i128 %102, %108
  %110 = trunc i128 %109 to i32
  %111 = icmp eq i32 %107, 128
  %112 = shl nsw i128 -1, %108
  %113 = xor i128 %112, -1
  %114 = select i1 %111, i128 -1, i128 %113
  %115 = and i128 %114, %102
  %116 = getelementptr inbounds i8, ptr %2, i64 20
  %117 = load i32, ptr %116, align 4, !tbaa !16
  %118 = shl nsw i32 %117, 1
  %119 = or i32 %118, 1
  %120 = icmp eq i32 %119, %110
  br i1 %120, label %121, label %125

121:                                              ; preds = %105
  %122 = icmp eq i128 %115, 0
  %123 = select i1 %122, i32 1, i32 2
  %124 = getelementptr inbounds i8, ptr %0, i64 5612
  store i32 %123, ptr %124, align 4, !tbaa !23
  br label %337

125:                                              ; preds = %105
  %126 = icmp eq i32 %110, 0
  %127 = add i32 %117, %107
  %128 = sub i32 %110, %127
  %129 = getelementptr inbounds i8, ptr %2, i64 8
  %130 = load i32, ptr %129, align 8
  %131 = select i1 %126, i32 %130, i32 %128
  %132 = getelementptr inbounds i8, ptr %0, i64 5604
  store i32 %131, ptr %132, align 4, !tbaa !35
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %5) #14
  %133 = shl nuw i128 1, %108
  %134 = select i1 %126, i128 0, i128 %133
  %135 = or i128 %134, %115
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %5, i8 0, i64 5604, i1 false), !alias.scope !50
  %136 = icmp eq i128 %135, 0
  br i1 %136, label %148, label %137

137:                                              ; preds = %125
  %138 = getelementptr inbounds i8, ptr %5, i64 4
  br label %139

139:                                              ; preds = %139, %137
  %140 = phi i128 [ %135, %137 ], [ %146, %139 ]
  %141 = trunc i128 %140 to i32
  %142 = load i32, ptr %5, align 4, !tbaa !34, !alias.scope !50
  %143 = add nsw i32 %142, 1
  store i32 %143, ptr %5, align 4, !tbaa !34, !alias.scope !50
  %144 = sext i32 %142 to i64
  %145 = getelementptr inbounds [1400 x i32], ptr %138, i64 0, i64 %144
  store i32 %141, ptr %145, align 4, !tbaa !24, !alias.scope !50
  %146 = lshr i128 %140, 32
  %147 = icmp ult i128 %140, 4294967296
  br i1 %147, label %148, label %139
148:                                              ; preds = %139, %125
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %0, ptr noundef nonnull align 4 dereferenceable(5604) %5, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %5) #14
  br label %337

149:                                              ; preds = %97
  %150 = add nsw i32 %9, -6
  %151 = zext i32 %150 to i128
  %152 = lshr i128 %102, %151
  %153 = trunc i128 %152 to i32
  %154 = icmp sgt i32 %153, 29
  br i1 %154, label %155, label %159

155:                                              ; preds = %149
  %156 = icmp eq i32 %153, 30
  %157 = select i1 %156, i32 1, i32 2
  %158 = getelementptr inbounds i8, ptr %0, i64 5612
  store i32 %157, ptr %158, align 4, !tbaa !23
  br label %337

159:                                              ; preds = %149
  %160 = add nsw i32 %9, -3
  %161 = zext i32 %160 to i128
  %162 = lshr i128 %102, %161
  %163 = icmp eq i128 %162, 3
  %164 = icmp ne i32 %9, 128
  %165 = and i1 %164, %163
  %166 = getelementptr inbounds i8, ptr %2, i64 16
  %167 = load i32, ptr %166, align 8, !tbaa !15
  br i1 %165, label %168, label %194

168:                                              ; preds = %159
  %169 = add nsw i32 %167, -2
  %170 = zext i32 %169 to i128
  %171 = lshr i128 %102, %170
  %172 = xor i32 %167, -1
  %173 = add i32 %9, %172
  %174 = icmp eq i32 %173, 128
  %175 = zext i32 %173 to i128
  %176 = shl nsw i128 -1, %175
  %177 = trunc i128 %176 to i64
  %178 = xor i64 %177, -1
  %179 = zext i64 %178 to i128
  %180 = select i1 %174, i128 4294967295, i128 %179
  %181 = and i128 %180, %171
  %182 = trunc i128 %181 to i32
  %183 = icmp eq i32 %169, 128
  %184 = shl nsw i128 -1, %170
  %185 = xor i128 %184, -1
  %186 = select i1 %183, i128 -1, i128 %185
  %187 = and i128 %186, %102
  %188 = zext i32 %167 to i128
  %189 = shl nuw i128 1, %188
  %190 = or i128 %187, %189
  %191 = trunc i128 %190 to i64
  %192 = lshr i128 %190, 64
  %193 = trunc i128 %192 to i64
  br label %206

194:                                              ; preds = %159
  %195 = zext i32 %167 to i128
  %196 = lshr i128 %102, %195
  %197 = trunc i128 %196 to i32
  %198 = icmp eq i32 %167, 128
  %199 = shl nsw i128 -1, %195
  %200 = xor i128 %199, -1
  %201 = select i1 %198, i128 -1, i128 %200
  %202 = and i128 %201, %102
  %203 = trunc i128 %202 to i64
  %204 = lshr i128 %202, 64
  %205 = trunc i128 %204 to i64
  br label %206

206:                                              ; preds = %194, %168
  %207 = phi i64 [ %203, %194 ], [ %191, %168 ]
  %208 = phi i64 [ %205, %194 ], [ %193, %168 ]
  %209 = phi i32 [ %197, %194 ], [ %182, %168 ]
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %6) #14
  %210 = zext i64 %208 to i128
  %211 = shl nuw i128 %210, 64
  %212 = zext i64 %207 to i128
  %213 = or i128 %211, %212
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %6, i8 0, i64 5604, i1 false), !alias.scope !53
  %214 = icmp eq i128 %213, 0
  br i1 %214, label %226, label %215

215:                                              ; preds = %206
  %216 = getelementptr inbounds i8, ptr %6, i64 4
  br label %217

217:                                              ; preds = %217, %215
  %218 = phi i128 [ %213, %215 ], [ %224, %217 ]
  %219 = trunc i128 %218 to i32
  %220 = load i32, ptr %6, align 4, !tbaa !34, !alias.scope !53
  %221 = add nsw i32 %220, 1
  store i32 %221, ptr %6, align 4, !tbaa !34, !alias.scope !53
  %222 = sext i32 %220 to i64
  %223 = getelementptr inbounds [1400 x i32], ptr %216, i64 0, i64 %222
  store i32 %219, ptr %223, align 4, !tbaa !24, !alias.scope !53
  %224 = lshr i128 %218, 32
  %225 = icmp ult i128 %218, 4294967296
  br i1 %225, label %226, label %217
226:                                              ; preds = %217, %206
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %0, ptr noundef nonnull align 4 dereferenceable(5604) %6, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %6) #14
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %7) #14
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %7, i8 0, i64 5604, i1 false), !alias.scope !56
  %227 = getelementptr inbounds i8, ptr %7, i64 4
  store i32 1, ptr %7, align 4, !tbaa !34, !alias.scope !56
  store i32 1, ptr %227, align 4, !tbaa !24, !alias.scope !56
  %228 = getelementptr inbounds i8, ptr %2, i64 4
  %229 = load i32, ptr %228, align 4, !tbaa !12
  %230 = icmp sgt i32 %229, 8
  br i1 %230, label %234, label %231

231:                                              ; preds = %263, %226
  %232 = phi i32 [ %229, %226 ], [ %264, %263 ]
  %233 = icmp sgt i32 %232, 0
  br i1 %233, label %266, label %298

234:                                              ; preds = %226, %263
  %235 = phi i32 [ %264, %263 ], [ %229, %226 ]
  %236 = load i32, ptr %7, align 4, !tbaa !34
  %237 = icmp sgt i32 %236, 0
  br i1 %237, label %238, label %240

238:                                              ; preds = %234
  %239 = zext i32 %236 to i64
  br label %243

240:                                              ; preds = %243, %234
  %241 = phi i64 [ 0, %234 ], [ %252, %243 ]
  %242 = icmp eq i64 %241, 0
  br i1 %242, label %263, label %255

243:                                              ; preds = %243, %238
  %244 = phi i64 [ 0, %238 ], [ %253, %243 ]
  %245 = phi i64 [ 0, %238 ], [ %252, %243 ]
  %246 = getelementptr inbounds [1400 x i32], ptr %227, i64 0, i64 %244
  %247 = load i32, ptr %246, align 4, !tbaa !24
  %248 = zext i32 %247 to i64
  %249 = mul nuw nsw i64 %248, 1000000000
  %250 = add nuw nsw i64 %249, %245
  %251 = trunc i64 %250 to i32
  store i32 %251, ptr %246, align 4, !tbaa !24
  %252 = lshr i64 %250, 32
  %253 = add nuw nsw i64 %244, 1
  %254 = icmp eq i64 %253, %239
  br i1 %254, label %240, label %243
255:                                              ; preds = %240
  %256 = icmp eq i32 %236, 1400
  br i1 %256, label %257, label %258

257:                                              ; preds = %255
  tail call void @llvm.trap()
  unreachable

258:                                              ; preds = %255
  %259 = trunc i64 %241 to i32
  %260 = add nsw i32 %236, 1
  store i32 %260, ptr %7, align 4, !tbaa !34
  %261 = sext i32 %236 to i64
  %262 = getelementptr inbounds [1400 x i32], ptr %227, i64 0, i64 %261
  store i32 %259, ptr %262, align 4, !tbaa !24
  br label %263

263:                                              ; preds = %258, %240
  %264 = add nsw i32 %235, -9
  %265 = icmp sgt i32 %235, 17
  br i1 %265, label %234, label %231
266:                                              ; preds = %231, %296
  %267 = phi i32 [ %268, %296 ], [ %232, %231 ]
  %268 = add nsw i32 %267, -1
  %269 = load i32, ptr %7, align 4, !tbaa !34
  %270 = icmp sgt i32 %269, 0
  br i1 %270, label %271, label %273

271:                                              ; preds = %266
  %272 = zext i32 %269 to i64
  br label %276

273:                                              ; preds = %276, %266
  %274 = phi i64 [ 0, %266 ], [ %285, %276 ]
  %275 = icmp eq i64 %274, 0
  br i1 %275, label %296, label %288

276:                                              ; preds = %276, %271
  %277 = phi i64 [ 0, %271 ], [ %286, %276 ]
  %278 = phi i64 [ 0, %271 ], [ %285, %276 ]
  %279 = getelementptr inbounds [1400 x i32], ptr %227, i64 0, i64 %277
  %280 = load i32, ptr %279, align 4, !tbaa !24
  %281 = zext i32 %280 to i64
  %282 = mul nuw nsw i64 %281, 10
  %283 = add nuw nsw i64 %282, %278
  %284 = trunc i64 %283 to i32
  store i32 %284, ptr %279, align 4, !tbaa !24
  %285 = lshr i64 %283, 32
  %286 = add nuw nsw i64 %277, 1
  %287 = icmp eq i64 %286, %272
  br i1 %287, label %273, label %276
288:                                              ; preds = %273
  %289 = icmp eq i32 %269, 1400
  br i1 %289, label %290, label %291

290:                                              ; preds = %288
  tail call void @llvm.trap()
  unreachable

291:                                              ; preds = %288
  %292 = trunc i64 %274 to i32
  %293 = add nsw i32 %269, 1
  store i32 %293, ptr %7, align 4, !tbaa !34
  %294 = sext i32 %269 to i64
  %295 = getelementptr inbounds [1400 x i32], ptr %227, i64 0, i64 %294
  store i32 %292, ptr %295, align 4, !tbaa !24
  br label %296

296:                                              ; preds = %291, %273
  %297 = icmp sgt i32 %267, 1
  br i1 %297, label %266, label %298
298:                                              ; preds = %296, %231
  %299 = load i32, ptr %0, align 4, !tbaa !34
  %300 = load i32, ptr %7, align 4, !tbaa !34
  %301 = icmp eq i32 %299, %300
  br i1 %301, label %302, label %307

302:                                              ; preds = %298
  %303 = getelementptr inbounds i8, ptr %0, i64 4
  %304 = icmp eq i32 %299, 0
  br i1 %304, label %323, label %305

305:                                              ; preds = %302
  %306 = sext i32 %299 to i64
  br label %312

307:                                              ; preds = %298
  %308 = icmp slt i32 %299, %300
  %309 = select i1 %308, i32 -1, i32 1
  br label %323

310:                                              ; preds = %312
  %311 = icmp eq i64 %314, 0
  br i1 %311, label %323, label %312
312:                                              ; preds = %305, %310
  %313 = phi i64 [ %306, %305 ], [ %314, %310 ]
  %314 = add nsw i64 %313, -1
  %315 = getelementptr inbounds [1400 x i32], ptr %303, i64 0, i64 %314
  %316 = load i32, ptr %315, align 4, !tbaa !24
  %317 = getelementptr inbounds [1400 x i32], ptr %227, i64 0, i64 %314
  %318 = load i32, ptr %317, align 4, !tbaa !24
  %319 = icmp eq i32 %316, %318
  br i1 %319, label %310, label %320
320:                                              ; preds = %312
  %321 = icmp ult i32 %316, %318
  %322 = select i1 %321, i32 -1, i32 1
  br label %323

323:                                              ; preds = %310, %302, %307, %320
  %324 = phi i32 [ %309, %307 ], [ %322, %320 ], [ 0, %302 ], [ 0, %310 ]
  %325 = icmp sgt i32 %324, -1
  br i1 %325, label %331, label %326

326:                                              ; preds = %323
  %327 = icmp eq i32 %9, 128
  %328 = and i128 %102, -42535295865117307932921825928971026432
  %329 = icmp eq i128 %328, 127605887595351923798765477786913079296
  %330 = select i1 %327, i1 %329, i1 false
  br i1 %330, label %331, label %332

331:                                              ; preds = %326, %323
  tail call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %0, i8 0, i64 5604, i1 false)
  br label %332

332:                                              ; preds = %331, %326
  %333 = getelementptr inbounds i8, ptr %2, i64 20
  %334 = load i32, ptr %333, align 4, !tbaa !16
  %335 = sub nsw i32 %209, %334
  %336 = getelementptr inbounds i8, ptr %0, i64 5604
  store i32 %335, ptr %336, align 4, !tbaa !35
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %7) #14
  br label %337

337:                                              ; preds = %148, %121, %332, %155, %96
  ret void
}

; Function Attrs: mustprogress nocallback nofree nounwind willreturn memory(argmem: readwrite)
declare void @llvm.memcpy.p0.p0.i64(ptr noalias writeonly, ptr noalias readonly, i64, i1 immarg) #3

; Function Attrs: mustprogress nocallback nofree nosync nounwind willreturn memory(argmem: readwrite)
declare void @llvm.lifetime.end.p0(i64 immarg, ptr) #1

; Function Attrs: nofree norecurse nosync nounwind memory(argmem: write)
define internal fastcc void @store(ptr noundef writeonly %0, i64 noundef %1, i64 noundef %2, i32 noundef %3) unnamed_addr #4 {
  %5 = zext i64 %2 to i128
  %6 = shl nuw i128 %5, 64
  %7 = zext i64 %1 to i128
  %8 = or i128 %6, %7
  switch i32 %3, label %9 [
    i32 8, label %14
    i32 16, label %16
    i32 32, label %18
    i32 64, label %20
    i32 128, label %21
  ]

9:                                                ; preds = %4
  %10 = icmp sgt i32 %3, 7
  br i1 %10, label %11, label %30

11:                                               ; preds = %9
  %12 = lshr i32 %3, 3
  %13 = zext i32 %12 to i64
  br label %22

14:                                               ; preds = %4
  %15 = trunc i64 %1 to i8
  store i8 %15, ptr %0, align 1, !tbaa !26
  br label %30

16:                                               ; preds = %4
  %17 = trunc i64 %1 to i16
  store i16 %17, ptr %0, align 1
  br label %30

18:                                               ; preds = %4
  %19 = trunc i64 %1 to i32
  store i32 %19, ptr %0, align 1
  br label %30

20:                                               ; preds = %4
  store i64 %1, ptr %0, align 1
  br label %30

21:                                               ; preds = %4
  store i128 %8, ptr %0, align 1
  br label %30

22:                                               ; preds = %11, %22
  %23 = phi i64 [ 0, %11 ], [ %28, %22 ]
  %24 = phi i128 [ %8, %11 ], [ %27, %22 ]
  %25 = trunc i128 %24 to i8
  %26 = getelementptr inbounds i8, ptr %0, i64 %23
  store i8 %25, ptr %26, align 1, !tbaa !26
  %27 = lshr i128 %24, 8
  %28 = add nuw nsw i64 %23, 1
  %29 = icmp eq i64 %28, %13
  br i1 %29, label %30, label %22
30:                                               ; preds = %22, %9, %14, %16, %18, %20, %21
  ret void
}

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
  %46 = tail call fastcc i32 @magnitude(ptr noundef %0, ptr noundef %1, i32 noundef %20) #15
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
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %9) #14
  %83 = sub nsw i32 %2, %54
  tail call void @llvm.experimental.noalias.scope.decl(metadata !65)
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %6) #14, !noalias !65
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %6, ptr noundef nonnull readonly align 4 dereferenceable(5604) %0, i64 5604, i1 false), !tbaa.struct !30, !noalias !65
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %7) #14, !noalias !65
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %7, ptr noundef nonnull readonly align 4 dereferenceable(5604) %1, i64 5604, i1 false), !tbaa.struct !30, !noalias !65
  %84 = icmp sgt i32 %83, -1
  br i1 %84, label %85, label %86

85:                                               ; preds = %82
  call fastcc void @power(ptr noundef %6, i32 noundef %20, i32 noundef %83) #15, !noalias !65
  br label %88

86:                                               ; preds = %82
  %87 = sub nsw i32 0, %83
  call fastcc void @power(ptr noundef %7, i32 noundef %20, i32 noundef %87) #15, !noalias !65
  br label %88

88:                                               ; preds = %86, %85
  call fastcc void @divide(ptr nonnull sret(%struct.tzrt_big) align 4 %9, ptr noundef %6, ptr noundef %7) #15
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
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %8) #14, !noalias !65
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
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %8) #14, !noalias !65
  br label %197

197:                                              ; preds = %147, %152, %196
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %7) #14, !noalias !65
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %6) #14, !noalias !65
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %10) #14
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %10, i8 0, i64 5604, i1 false), !alias.scope !71
  %198 = getelementptr inbounds i8, ptr %10, i64 4
  store i32 1, ptr %10, align 4, !tbaa !34, !alias.scope !71
  store i32 1, ptr %198, align 4, !tbaa !24, !alias.scope !71
  call fastcc void @power(ptr noundef %10, i32 noundef %20, i32 noundef %48) #15
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
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %11) #14
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
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %11) #14
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
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %10) #14
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %9) #14
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
  call void @llvm.lifetime.start.p0(i64 36, ptr nonnull %4) #14
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
  call void @llvm.lifetime.start.p0(i64 5616, ptr nonnull %5) #14
  call fastcc void @decode(ptr sret(%struct.tzrt_number) align 4 %5, ptr noundef %0, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %4) #15
  call void @llvm.lifetime.start.p0(i64 5616, ptr nonnull %6) #14
  call fastcc void @decode(ptr sret(%struct.tzrt_number) align 4 %6, ptr noundef %1, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %4) #15
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
  call fastcc void @power(ptr noundef %5, i32 noundef %84, i32 noundef %120) #15
  %121 = sub nsw i32 %118, %119
  call fastcc void @power(ptr noundef %6, i32 noundef %84, i32 noundef %121) #15
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
  call void @llvm.lifetime.end.p0(i64 5616, ptr nonnull %6) #14
  call void @llvm.lifetime.end.p0(i64 5616, ptr nonnull %5) #14
  call void @llvm.lifetime.end.p0(i64 36, ptr nonnull %4) #14
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
  call void @llvm.lifetime.start.p0(i64 36, ptr nonnull %5) #14
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
  call void @llvm.lifetime.start.p0(i64 36, ptr nonnull %6) #14
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
  call void @llvm.lifetime.start.p0(i64 5616, ptr nonnull %7) #14
  call fastcc void @decode(ptr sret(%struct.tzrt_number) align 4 %7, ptr noundef %1, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %5) #15
  br i1 %170, label %389, label %176

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
  br i1 %197, label %222, label %198

198:                                              ; preds = %176
  %199 = icmp eq i32 %185, 0
  %200 = sub i128 0, %196
  %201 = select i1 %199, i128 %196, i128 %200
  switch i32 %171, label %202 [
    i32 8, label %205
    i32 16, label %207
    i32 32, label %209
    i32 64, label %211
    i32 128, label %213
  ]

202:                                              ; preds = %198
  %203 = lshr exact i32 %171, 3
  %204 = zext i32 %203 to i64
  br label %214

205:                                              ; preds = %198
  %206 = trunc i128 %201 to i8
  store i8 %206, ptr %0, align 1, !tbaa !26
  br label %497

207:                                              ; preds = %198
  %208 = trunc i128 %201 to i16
  store i16 %208, ptr %0, align 1
  br label %497

209:                                              ; preds = %198
  %210 = trunc i128 %201 to i32
  store i32 %210, ptr %0, align 1
  br label %497

211:                                              ; preds = %198
  %212 = trunc i128 %201 to i64
  store i64 %212, ptr %0, align 1
  br label %497

213:                                              ; preds = %198
  store i128 %201, ptr %0, align 1
  br label %497

214:                                              ; preds = %214, %202
  %215 = phi i64 [ 0, %202 ], [ %220, %214 ]
  %216 = phi i128 [ %201, %202 ], [ %219, %214 ]
  %217 = trunc i128 %216 to i8
  %218 = getelementptr inbounds i8, ptr %0, i64 %215
  store i8 %217, ptr %218, align 1, !tbaa !26
  %219 = lshr i128 %216, 8
  %220 = add nuw nsw i64 %215, 1
  %221 = icmp eq i64 %220, %204
  br i1 %221, label %497, label %214
222:                                              ; preds = %176
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %8) #14
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %8, ptr noundef nonnull align 4 dereferenceable(5604) %7, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %9) #14
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %9, i8 0, i64 5604, i1 false), !alias.scope !85
  %223 = getelementptr inbounds i8, ptr %9, i64 4
  store i32 1, ptr %9, align 4, !tbaa !34, !alias.scope !85
  store i32 1, ptr %223, align 4, !tbaa !24, !alias.scope !85
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %10) #14
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %10, i8 0, i64 5604, i1 false), !alias.scope !88
  %224 = icmp eq i128 %196, 0
  br i1 %224, label %236, label %225

225:                                              ; preds = %222
  %226 = getelementptr inbounds i8, ptr %10, i64 4
  br label %227

227:                                              ; preds = %227, %225
  %228 = phi i128 [ %196, %225 ], [ %234, %227 ]
  %229 = trunc i128 %228 to i32
  %230 = load i32, ptr %10, align 4, !tbaa !34, !alias.scope !88
  %231 = add nsw i32 %230, 1
  store i32 %231, ptr %10, align 4, !tbaa !34, !alias.scope !88
  %232 = sext i32 %230 to i64
  %233 = getelementptr inbounds [1400 x i32], ptr %226, i64 0, i64 %232
  store i32 %229, ptr %233, align 4, !tbaa !24, !alias.scope !88
  %234 = lshr i128 %228, 32
  %235 = icmp ult i128 %228, 4294967296
  br i1 %235, label %236, label %227
236:                                              ; preds = %227, %222
  %237 = getelementptr inbounds i8, ptr %7, i64 5604
  %238 = load i32, ptr %237, align 4, !tbaa !35
  %239 = icmp sgt i32 %238, -1
  br i1 %239, label %240, label %241

240:                                              ; preds = %236
  call fastcc void @power(ptr noundef %8, i32 noundef %91, i32 noundef %238) #15
  br label %243

241:                                              ; preds = %236
  %242 = sub nsw i32 0, %238
  call fastcc void @power(ptr noundef %9, i32 noundef %91, i32 noundef %242) #15
  br label %243

243:                                              ; preds = %241, %240
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %11) #14
  tail call void @llvm.experimental.noalias.scope.decl(metadata !91)
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %11, i8 0, i64 5604, i1 false), !alias.scope !91
  %244 = load i32, ptr %10, align 4, !tbaa !34, !noalias !91
  %245 = load i32, ptr %9, align 4, !tbaa !34, !noalias !91
  %246 = add nsw i32 %245, %244
  %247 = icmp sgt i32 %246, 1400
  br i1 %247, label %248, label %249

248:                                              ; preds = %243
  tail call void @llvm.trap()
  unreachable

249:                                              ; preds = %243
  store i32 %246, ptr %11, align 4, !tbaa !34, !alias.scope !91
  %250 = icmp sgt i32 %244, 0
  br i1 %250, label %251, label %264

251:                                              ; preds = %249
  %252 = icmp sgt i32 %245, 0
  %253 = getelementptr inbounds i8, ptr %10, i64 4
  %254 = getelementptr inbounds i8, ptr %11, i64 4
  %255 = sext i32 %245 to i64
  %256 = zext i32 %244 to i64
  %257 = zext i32 %245 to i64
  br label %258

258:                                              ; preds = %281, %251
  %259 = phi i64 [ 0, %251 ], [ %285, %281 ]
  br i1 %252, label %260, label %281

260:                                              ; preds = %258
  %261 = getelementptr inbounds [1400 x i32], ptr %253, i64 0, i64 %259
  %262 = load i32, ptr %261, align 4, !tbaa !24, !noalias !91
  %263 = zext i32 %262 to i64
  br label %287

264:                                              ; preds = %281, %249
  %265 = getelementptr inbounds i8, ptr %11, i64 4
  %266 = load i32, ptr %11, align 4, !tbaa !34, !alias.scope !91
  %267 = icmp eq i32 %266, 0
  br i1 %267, label %304, label %268

268:                                              ; preds = %264
  %269 = sext i32 %266 to i64
  br label %270

270:                                              ; preds = %276, %268
  %271 = phi i64 [ %269, %268 ], [ %272, %276 ]
  %272 = add nsw i64 %271, -1
  %273 = getelementptr inbounds [1400 x i32], ptr %265, i64 0, i64 %272
  %274 = load i32, ptr %273, align 4, !tbaa !24, !alias.scope !91
  %275 = icmp eq i32 %274, 0
  br i1 %275, label %276, label %304

276:                                              ; preds = %270
  %277 = trunc i64 %272 to i32
  store i32 %277, ptr %11, align 4, !tbaa !34, !alias.scope !91
  %278 = icmp eq i64 %272, 0
  br i1 %278, label %304, label %270
279:                                              ; preds = %287
  %280 = trunc i64 %301 to i32
  br label %281

281:                                              ; preds = %279, %258
  %282 = phi i32 [ 0, %258 ], [ %280, %279 ]
  %283 = add nsw i64 %259, %255
  %284 = getelementptr inbounds [1400 x i32], ptr %254, i64 0, i64 %283
  store i32 %282, ptr %284, align 4, !tbaa !24, !alias.scope !91
  %285 = add nuw nsw i64 %259, 1
  %286 = icmp eq i64 %285, %256
  br i1 %286, label %264, label %258
287:                                              ; preds = %287, %260
  %288 = phi i64 [ 0, %260 ], [ %302, %287 ]
  %289 = phi i64 [ 0, %260 ], [ %301, %287 ]
  %290 = getelementptr inbounds [1400 x i32], ptr %223, i64 0, i64 %288
  %291 = load i32, ptr %290, align 4, !tbaa !24, !noalias !91
  %292 = zext i32 %291 to i64
  %293 = mul nuw i64 %292, %263
  %294 = add nuw nsw i64 %288, %259
  %295 = getelementptr inbounds [1400 x i32], ptr %254, i64 0, i64 %294
  %296 = load i32, ptr %295, align 4, !tbaa !24, !alias.scope !91
  %297 = zext i32 %296 to i64
  %298 = add nuw nsw i64 %289, %297
  %299 = add nuw i64 %298, %293
  %300 = trunc i64 %299 to i32
  store i32 %300, ptr %295, align 4, !tbaa !24, !alias.scope !91
  %301 = lshr i64 %299, 32
  %302 = add nuw nsw i64 %288, 1
  %303 = icmp eq i64 %302, %257
  br i1 %303, label %279, label %287
304:                                              ; preds = %270, %276, %264
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %10, ptr noundef nonnull align 4 dereferenceable(5604) %11, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %11) #14
  %305 = load i32, ptr %8, align 4, !tbaa !34
  %306 = load i32, ptr %10, align 4, !tbaa !34
  %307 = icmp eq i32 %305, %306
  br i1 %307, label %308, label %314

308:                                              ; preds = %304
  %309 = getelementptr inbounds i8, ptr %8, i64 4
  %310 = getelementptr inbounds i8, ptr %10, i64 4
  %311 = icmp eq i32 %305, 0
  br i1 %311, label %330, label %312

312:                                              ; preds = %308
  %313 = sext i32 %305 to i64
  br label %319

314:                                              ; preds = %304
  %315 = icmp slt i32 %305, %306
  %316 = select i1 %315, i32 -1, i32 1
  br label %330

317:                                              ; preds = %319
  %318 = icmp eq i64 %321, 0
  br i1 %318, label %330, label %319
319:                                              ; preds = %312, %317
  %320 = phi i64 [ %313, %312 ], [ %321, %317 ]
  %321 = add nsw i64 %320, -1
  %322 = getelementptr inbounds [1400 x i32], ptr %309, i64 0, i64 %321
  %323 = load i32, ptr %322, align 4, !tbaa !24
  %324 = getelementptr inbounds [1400 x i32], ptr %310, i64 0, i64 %321
  %325 = load i32, ptr %324, align 4, !tbaa !24
  %326 = icmp eq i32 %323, %325
  br i1 %326, label %317, label %327
327:                                              ; preds = %319
  %328 = icmp ult i32 %323, %325
  %329 = select i1 %328, i32 -1, i32 1
  br label %330

330:                                              ; preds = %317, %308, %314, %327
  %331 = phi i32 [ %316, %314 ], [ %329, %327 ], [ 0, %308 ], [ 0, %317 ]
  %332 = icmp sgt i32 %331, -1
  br i1 %332, label %363, label %333

333:                                              ; preds = %330
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %12) #14
  call fastcc void @divide(ptr sret(%struct.tzrt_big) align 4 %12, ptr noundef %8, ptr noundef %9) #15
  %334 = load i32, ptr %12, align 4, !tbaa !34
  %335 = icmp sgt i32 %334, 4
  br i1 %335, label %341, label %336

336:                                              ; preds = %333
  %337 = icmp eq i32 %334, 0
  br i1 %337, label %356, label %338

338:                                              ; preds = %336
  %339 = getelementptr inbounds i8, ptr %12, i64 4
  %340 = sext i32 %334 to i64
  br label %346

341:                                              ; preds = %333
  tail call void @llvm.trap()
  unreachable

342:                                              ; preds = %346
  %343 = lshr i128 %350, 64
  %344 = trunc i128 %343 to i64
  %345 = trunc i128 %354 to i64
  br label %356

346:                                              ; preds = %346, %338
  %347 = phi i64 [ %340, %338 ], [ %349, %346 ]
  %348 = phi i128 [ 0, %338 ], [ %354, %346 ]
  %349 = add nsw i64 %347, -1
  %350 = shl i128 %348, 32
  %351 = getelementptr inbounds [1400 x i32], ptr %339, i64 0, i64 %349
  %352 = load i32, ptr %351, align 4, !tbaa !24
  %353 = zext i32 %352 to i128
  %354 = or i128 %350, %353
  %355 = icmp eq i64 %349, 0
  br i1 %355, label %342, label %346
356:                                              ; preds = %336, %342
  %357 = phi i64 [ 0, %336 ], [ %345, %342 ]
  %358 = phi i64 [ 0, %336 ], [ %344, %342 ]
  %359 = zext i64 %358 to i128
  %360 = shl nuw i128 %359, 64
  %361 = zext i64 %357 to i128
  %362 = or i128 %360, %361
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %12) #14
  br label %363

363:                                              ; preds = %330, %356
  %364 = phi i128 [ %362, %356 ], [ %196, %330 ]
  %365 = icmp eq i32 %185, 0
  %366 = sub i128 0, %364
  %367 = select i1 %365, i128 %364, i128 %366
  switch i32 %171, label %368 [
    i32 8, label %371
    i32 16, label %373
    i32 32, label %375
    i32 64, label %377
    i32 128, label %379
  ]

368:                                              ; preds = %363
  %369 = lshr exact i32 %171, 3
  %370 = zext i32 %369 to i64
  br label %380

371:                                              ; preds = %363
  %372 = trunc i128 %367 to i8
  store i8 %372, ptr %0, align 1, !tbaa !26
  br label %388

373:                                              ; preds = %363
  %374 = trunc i128 %367 to i16
  store i16 %374, ptr %0, align 1
  br label %388

375:                                              ; preds = %363
  %376 = trunc i128 %367 to i32
  store i32 %376, ptr %0, align 1
  br label %388

377:                                              ; preds = %363
  %378 = trunc i128 %367 to i64
  store i64 %378, ptr %0, align 1
  br label %388

379:                                              ; preds = %363
  store i128 %367, ptr %0, align 1
  br label %388

380:                                              ; preds = %380, %368
  %381 = phi i64 [ 0, %368 ], [ %386, %380 ]
  %382 = phi i128 [ %367, %368 ], [ %385, %380 ]
  %383 = trunc i128 %382 to i8
  %384 = getelementptr inbounds i8, ptr %0, i64 %381
  store i8 %383, ptr %384, align 1, !tbaa !26
  %385 = lshr i128 %382, 8
  %386 = add nuw nsw i64 %381, 1
  %387 = icmp eq i64 %386, %370
  br i1 %387, label %388, label %380
388:                                              ; preds = %380, %371, %373, %375, %377, %379
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %10) #14
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %9) #14
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %8) #14
  br label %497

389:                                              ; preds = %168
  %390 = getelementptr inbounds i8, ptr %7, i64 5612
  %391 = load i32, ptr %390, align 4, !tbaa !23
  %392 = icmp eq i32 %391, 0
  br i1 %392, label %454, label %393

393:                                              ; preds = %389
  %394 = getelementptr inbounds i8, ptr %7, i64 5608
  %395 = load i32, ptr %394, align 4, !tbaa !20
  %396 = sext i32 %395 to i128
  %397 = add nsw i32 %171, -1
  %398 = zext i32 %397 to i128
  %399 = shl i128 %396, %398
  br i1 %174, label %400, label %417

400:                                              ; preds = %393
  %401 = zext i32 %172 to i128
  %402 = zext i32 %173 to i128
  %403 = shl nuw i128 %401, %402
  %404 = or i128 %399, %403
  %405 = icmp eq i32 %391, 2
  %406 = trunc i128 %404 to i64
  %407 = lshr i128 %404, 64
  %408 = trunc i128 %407 to i64
  br i1 %405, label %409, label %428

409:                                              ; preds = %400
  %410 = add nsw i32 %173, -1
  %411 = zext i32 %410 to i128
  %412 = shl nuw i128 1, %411
  %413 = or i128 %404, %412
  %414 = trunc i128 %413 to i64
  %415 = lshr i128 %413, 64
  %416 = trunc i128 %415 to i64
  br label %428

417:                                              ; preds = %393
  %418 = icmp eq i32 %391, 2
  %419 = select i1 %418, i32 31, i32 30
  %420 = zext i32 %419 to i128
  %421 = add nsw i32 %171, -6
  %422 = zext i32 %421 to i128
  %423 = shl i128 %420, %422
  %424 = or i128 %399, %423
  %425 = trunc i128 %424 to i64
  %426 = lshr i128 %424, 64
  %427 = trunc i128 %426 to i64
  br label %428

428:                                              ; preds = %400, %409, %417
  %429 = phi i64 [ %414, %409 ], [ %406, %400 ], [ %425, %417 ]
  %430 = phi i64 [ %416, %409 ], [ %408, %400 ], [ %427, %417 ]
  %431 = zext i64 %430 to i128
  %432 = shl nuw i128 %431, 64
  %433 = zext i64 %429 to i128
  %434 = or i128 %432, %433
  switch i32 %171, label %435 [
    i32 8, label %438
    i32 16, label %440
    i32 32, label %442
    i32 64, label %444
    i32 128, label %445
  ]

435:                                              ; preds = %428
  %436 = lshr exact i32 %171, 3
  %437 = zext i32 %436 to i64
  br label %446

438:                                              ; preds = %428
  %439 = trunc i64 %429 to i8
  store i8 %439, ptr %0, align 1, !tbaa !26
  br label %497

440:                                              ; preds = %428
  %441 = trunc i64 %429 to i16
  store i16 %441, ptr %0, align 1
  br label %497

442:                                              ; preds = %428
  %443 = trunc i64 %429 to i32
  store i32 %443, ptr %0, align 1
  br label %497

444:                                              ; preds = %428
  store i64 %429, ptr %0, align 1
  br label %497

445:                                              ; preds = %428
  store i128 %434, ptr %0, align 1
  br label %497

446:                                              ; preds = %446, %435
  %447 = phi i64 [ 0, %435 ], [ %452, %446 ]
  %448 = phi i128 [ %434, %435 ], [ %451, %446 ]
  %449 = trunc i128 %448 to i8
  %450 = getelementptr inbounds i8, ptr %0, i64 %447
  store i8 %449, ptr %450, align 1, !tbaa !26
  %451 = lshr i128 %448, 8
  %452 = add nuw nsw i64 %447, 1
  %453 = icmp eq i64 %452, %437
  br i1 %453, label %497, label %446
454:                                              ; preds = %389
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %13) #14
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %13, i8 0, i64 5604, i1 false), !alias.scope !94
  %455 = getelementptr inbounds i8, ptr %13, i64 4
  store i32 1, ptr %13, align 4, !tbaa !34, !alias.scope !94
  store i32 1, ptr %455, align 4, !tbaa !24, !alias.scope !94
  %456 = icmp eq i32 %91, %175
  br i1 %456, label %465, label %457

457:                                              ; preds = %454
  %458 = getelementptr inbounds i8, ptr %7, i64 5604
  %459 = load i32, ptr %458, align 4, !tbaa !35
  %460 = icmp sgt i32 %459, -1
  br i1 %460, label %461, label %462

461:                                              ; preds = %457
  call fastcc void @power(ptr noundef %7, i32 noundef %91, i32 noundef %459) #15
  br label %464

462:                                              ; preds = %457
  %463 = sub nsw i32 0, %459
  call fastcc void @power(ptr noundef %13, i32 noundef %91, i32 noundef %463) #15
  br label %464

464:                                              ; preds = %462, %461
  store i32 0, ptr %458, align 4, !tbaa !35
  br label %465

465:                                              ; preds = %464, %454
  %466 = getelementptr inbounds i8, ptr %7, i64 5604
  %467 = load i32, ptr %466, align 4, !tbaa !35
  %468 = getelementptr inbounds i8, ptr %7, i64 5608
  %469 = load i32, ptr %468, align 4, !tbaa !20
  %470 = call fastcc { i64, i64 } @pack(ptr noundef %7, ptr noundef %13, i32 noundef %467, i32 noundef %469, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %6) #15
  %471 = extractvalue { i64, i64 } %470, 0
  %472 = extractvalue { i64, i64 } %470, 1
  %473 = zext i64 %472 to i128
  %474 = shl nuw i128 %473, 64
  %475 = zext i64 %471 to i128
  %476 = or i128 %474, %475
  switch i32 %171, label %477 [
    i32 8, label %480
    i32 16, label %482
    i32 32, label %484
    i32 64, label %486
    i32 128, label %487
  ]

477:                                              ; preds = %465
  %478 = lshr exact i32 %171, 3
  %479 = zext i32 %478 to i64
  br label %488

480:                                              ; preds = %465
  %481 = trunc i64 %471 to i8
  store i8 %481, ptr %0, align 1, !tbaa !26
  br label %496

482:                                              ; preds = %465
  %483 = trunc i64 %471 to i16
  store i16 %483, ptr %0, align 1
  br label %496

484:                                              ; preds = %465
  %485 = trunc i64 %471 to i32
  store i32 %485, ptr %0, align 1
  br label %496

486:                                              ; preds = %465
  store i64 %471, ptr %0, align 1
  br label %496

487:                                              ; preds = %465
  store i128 %476, ptr %0, align 1
  br label %496

488:                                              ; preds = %488, %477
  %489 = phi i64 [ 0, %477 ], [ %494, %488 ]
  %490 = phi i128 [ %476, %477 ], [ %493, %488 ]
  %491 = trunc i128 %490 to i8
  %492 = getelementptr inbounds i8, ptr %0, i64 %489
  store i8 %491, ptr %492, align 1, !tbaa !26
  %493 = lshr i128 %490, 8
  %494 = add nuw nsw i64 %489, 1
  %495 = icmp eq i64 %494, %479
  br i1 %495, label %496, label %488
496:                                              ; preds = %488, %480, %482, %484, %486, %487
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %13) #14
  br label %497

497:                                              ; preds = %214, %446, %445, %444, %442, %440, %438, %213, %211, %209, %207, %205, %388, %496
  call void @llvm.lifetime.end.p0(i64 5616, ptr nonnull %7) #14
  call void @llvm.lifetime.end.p0(i64 36, ptr nonnull %6) #14
  call void @llvm.lifetime.end.p0(i64 36, ptr nonnull %5) #14
  ret void
}

; Function Attrs: nounwind memory(argmem: readwrite, inaccessiblemem: write)
define internal fastcc void @divide(ptr noalias nonnull sret(%struct.tzrt_big) align 4 %0, ptr noundef nonnull %1, ptr noundef nonnull readonly %2) unnamed_addr #2 {
  %4 = alloca %struct.tzrt_big, align 4
  tail call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %0, i8 0, i64 5604, i1 false)
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %4) #14
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
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %4) #14
  ret void
}

; Function Attrs: nounwind memory(argmem: readwrite, inaccessiblemem: write)
define weak hidden i32 @tz_soft_format(ptr noundef writeonly %0, ptr noundef readonly %1, i32 noundef %2) local_unnamed_addr #2 {
  %4 = alloca %struct.tzrt_format, align 8
  %5 = alloca [40 x i8], align 16
  call void @llvm.lifetime.start.p0(i64 36, ptr nonnull %4) #14
  switch i32 %2, label %69 [
    i32 0, label %6
    i32 1, label %15
    i32 2, label %24
    i32 3, label %33
    i32 4, label %42
    i32 5, label %51
    i32 6, label %60
  ]

6:                                                ; preds = %3
  store i32 2, ptr %4, align 8, !tbaa !4, !alias.scope !99
  %7 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 11, ptr %7, align 4, !tbaa !12, !alias.scope !99
  %8 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 -24, ptr %8, align 8, !tbaa !13, !alias.scope !99
  %9 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 5, ptr %9, align 4, !tbaa !14, !alias.scope !99
  %10 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 10, ptr %10, align 8, !tbaa !15, !alias.scope !99
  %11 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 15, ptr %11, align 4, !tbaa !16, !alias.scope !99
  %12 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 16, ptr %12, align 8, !tbaa !17, !alias.scope !99
  %13 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 0, ptr %13, align 4, !tbaa !18, !alias.scope !99
  %14 = getelementptr inbounds i8, ptr %4, i64 32
  store i32 1, ptr %14, align 8, !tbaa !19, !alias.scope !99
  br label %82

15:                                               ; preds = %3
  store i32 2, ptr %4, align 8, !tbaa !4, !alias.scope !99
  %16 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 24, ptr %16, align 4, !tbaa !12, !alias.scope !99
  %17 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 -149, ptr %17, align 8, !tbaa !13, !alias.scope !99
  %18 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 104, ptr %18, align 4, !tbaa !14, !alias.scope !99
  %19 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 23, ptr %19, align 8, !tbaa !15, !alias.scope !99
  %20 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 127, ptr %20, align 4, !tbaa !16, !alias.scope !99
  %21 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 32, ptr %21, align 8, !tbaa !17, !alias.scope !99
  %22 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 0, ptr %22, align 4, !tbaa !18, !alias.scope !99
  %23 = getelementptr inbounds i8, ptr %4, i64 32
  store i32 1, ptr %23, align 8, !tbaa !19, !alias.scope !99
  br label %82

24:                                               ; preds = %3
  store i32 2, ptr %4, align 8, !tbaa !4, !alias.scope !99
  %25 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 53, ptr %25, align 4, !tbaa !12, !alias.scope !99
  %26 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 -1074, ptr %26, align 8, !tbaa !13, !alias.scope !99
  %27 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 971, ptr %27, align 4, !tbaa !14, !alias.scope !99
  %28 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 52, ptr %28, align 8, !tbaa !15, !alias.scope !99
  %29 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 1023, ptr %29, align 4, !tbaa !16, !alias.scope !99
  %30 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 64, ptr %30, align 8, !tbaa !17, !alias.scope !99
  %31 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 0, ptr %31, align 4, !tbaa !18, !alias.scope !99
  %32 = getelementptr inbounds i8, ptr %4, i64 32
  store i32 1, ptr %32, align 8, !tbaa !19, !alias.scope !99
  br label %82

33:                                               ; preds = %3
  store i32 2, ptr %4, align 8, !tbaa !4, !alias.scope !99
  %34 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 113, ptr %34, align 4, !tbaa !12, !alias.scope !99
  %35 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 -16494, ptr %35, align 8, !tbaa !13, !alias.scope !99
  %36 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 16271, ptr %36, align 4, !tbaa !14, !alias.scope !99
  %37 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 112, ptr %37, align 8, !tbaa !15, !alias.scope !99
  %38 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 16383, ptr %38, align 4, !tbaa !16, !alias.scope !99
  %39 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 128, ptr %39, align 8, !tbaa !17, !alias.scope !99
  %40 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 0, ptr %40, align 4, !tbaa !18, !alias.scope !99
  %41 = getelementptr inbounds i8, ptr %4, i64 32
  store i32 1, ptr %41, align 8, !tbaa !19, !alias.scope !99
  br label %82

42:                                               ; preds = %3
  store i32 10, ptr %4, align 8, !tbaa !4, !alias.scope !99
  %43 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 7, ptr %43, align 4, !tbaa !12, !alias.scope !99
  %44 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 -101, ptr %44, align 8, !tbaa !13, !alias.scope !99
  %45 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 90, ptr %45, align 4, !tbaa !14, !alias.scope !99
  %46 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 23, ptr %46, align 8, !tbaa !15, !alias.scope !99
  %47 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 101, ptr %47, align 4, !tbaa !16, !alias.scope !99
  %48 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 32, ptr %48, align 8, !tbaa !17, !alias.scope !99
  %49 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 0, ptr %49, align 4, !tbaa !18, !alias.scope !99
  %50 = getelementptr inbounds i8, ptr %4, i64 32
  store i32 1, ptr %50, align 8, !tbaa !19, !alias.scope !99
  br label %82

51:                                               ; preds = %3
  store i32 10, ptr %4, align 8, !tbaa !4, !alias.scope !99
  %52 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 16, ptr %52, align 4, !tbaa !12, !alias.scope !99
  %53 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 -398, ptr %53, align 8, !tbaa !13, !alias.scope !99
  %54 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 369, ptr %54, align 4, !tbaa !14, !alias.scope !99
  %55 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 53, ptr %55, align 8, !tbaa !15, !alias.scope !99
  %56 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 398, ptr %56, align 4, !tbaa !16, !alias.scope !99
  %57 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 64, ptr %57, align 8, !tbaa !17, !alias.scope !99
  %58 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 0, ptr %58, align 4, !tbaa !18, !alias.scope !99
  %59 = getelementptr inbounds i8, ptr %4, i64 32
  store i32 1, ptr %59, align 8, !tbaa !19, !alias.scope !99
  br label %82

60:                                               ; preds = %3
  store i32 10, ptr %4, align 8, !tbaa !4, !alias.scope !99
  %61 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 34, ptr %61, align 4, !tbaa !12, !alias.scope !99
  %62 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 -6176, ptr %62, align 8, !tbaa !13, !alias.scope !99
  %63 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 6111, ptr %63, align 4, !tbaa !14, !alias.scope !99
  %64 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 113, ptr %64, align 8, !tbaa !15, !alias.scope !99
  %65 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 6176, ptr %65, align 4, !tbaa !16, !alias.scope !99
  %66 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 128, ptr %66, align 8, !tbaa !17, !alias.scope !99
  %67 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 0, ptr %67, align 4, !tbaa !18, !alias.scope !99
  %68 = getelementptr inbounds i8, ptr %4, i64 32
  store i32 1, ptr %68, align 8, !tbaa !19, !alias.scope !99
  br label %82

69:                                               ; preds = %3
  %70 = and i32 %2, 7
  %71 = shl nuw nsw i32 8, %70
  store i32 2, ptr %4, align 8, !tbaa !4, !alias.scope !99
  %72 = getelementptr inbounds i8, ptr %4, i64 4
  store i32 %71, ptr %72, align 4, !tbaa !12, !alias.scope !99
  %73 = getelementptr inbounds i8, ptr %4, i64 8
  store i32 0, ptr %73, align 8, !tbaa !13, !alias.scope !99
  %74 = getelementptr inbounds i8, ptr %4, i64 12
  store i32 0, ptr %74, align 4, !tbaa !14, !alias.scope !99
  %75 = getelementptr inbounds i8, ptr %4, i64 16
  store i32 0, ptr %75, align 8, !tbaa !15, !alias.scope !99
  %76 = getelementptr inbounds i8, ptr %4, i64 20
  store i32 0, ptr %76, align 4, !tbaa !16, !alias.scope !99
  %77 = getelementptr inbounds i8, ptr %4, i64 24
  store i32 %71, ptr %77, align 8, !tbaa !17, !alias.scope !99
  %78 = getelementptr inbounds i8, ptr %4, i64 28
  store i32 1, ptr %78, align 4, !tbaa !18, !alias.scope !99
  %79 = getelementptr inbounds i8, ptr %4, i64 32
  %80 = icmp slt i32 %2, 24
  %81 = zext i1 %80 to i32
  store i32 %81, ptr %79, align 8, !tbaa !19, !alias.scope !99
  br label %82

82:                                               ; preds = %6, %15, %24, %33, %42, %51, %60, %69
  %83 = phi i1 [ true, %6 ], [ true, %15 ], [ true, %24 ], [ true, %33 ], [ true, %42 ], [ true, %51 ], [ true, %60 ], [ %80, %69 ]
  %84 = phi i1 [ true, %6 ], [ true, %15 ], [ true, %24 ], [ true, %33 ], [ true, %42 ], [ true, %51 ], [ true, %60 ], [ false, %69 ]
  %85 = phi i32 [ 16, %6 ], [ 32, %15 ], [ 64, %24 ], [ 128, %33 ], [ 32, %42 ], [ 64, %51 ], [ 128, %60 ], [ %71, %69 ]
  %86 = phi i1 [ true, %6 ], [ true, %15 ], [ true, %24 ], [ true, %33 ], [ false, %42 ], [ false, %51 ], [ false, %60 ], [ true, %69 ]
  br i1 %84, label %238, label %87

87:                                               ; preds = %82
  switch i32 %85, label %104 [
    i32 8, label %88
    i32 16, label %91
    i32 32, label %94
    i32 64, label %97
    i32 128, label %99
  ]

88:                                               ; preds = %87
  %89 = load i8, ptr %1, align 1, !tbaa !26
  %90 = zext i8 %89 to i64
  br label %124

91:                                               ; preds = %87
  %92 = load i16, ptr %1, align 1
  %93 = zext i16 %92 to i64
  br label %124

94:                                               ; preds = %87
  %95 = load i32, ptr %1, align 1
  %96 = zext i32 %95 to i64
  br label %124

97:                                               ; preds = %87
  %98 = load i64, ptr %1, align 1
  br label %124

99:                                               ; preds = %87
  %100 = load i128, ptr %1, align 1
  %101 = trunc i128 %100 to i64
  %102 = lshr i128 %100, 64
  %103 = trunc i128 %102 to i64
  br label %124

104:                                              ; preds = %87
  %105 = or i32 %85, 7
  %106 = icmp samesign ult i32 %105, 15
  br i1 %106, label %124, label %107

107:                                              ; preds = %104
  %108 = lshr exact i32 %85, 3
  %109 = zext i32 %108 to i64
  br label %114

110:                                              ; preds = %114
  %111 = lshr i128 %118, 64
  %112 = trunc i128 %111 to i64
  %113 = trunc i128 %122 to i64
  br label %124

114:                                              ; preds = %114, %107
  %115 = phi i64 [ %109, %107 ], [ %117, %114 ]
  %116 = phi i128 [ 0, %107 ], [ %122, %114 ]
  %117 = add nsw i64 %115, -1
  %118 = shl i128 %116, 8
  %119 = getelementptr inbounds i8, ptr %1, i64 %117
  %120 = load i8, ptr %119, align 1, !tbaa !26
  %121 = zext i8 %120 to i128
  %122 = or i128 %118, %121
  %123 = icmp eq i64 %117, 0
  br i1 %123, label %110, label %114
124:                                              ; preds = %88, %91, %94, %97, %99, %104, %110
  %125 = phi i64 [ %90, %88 ], [ %93, %91 ], [ %96, %94 ], [ %98, %97 ], [ %101, %99 ], [ 0, %104 ], [ %113, %110 ]
  %126 = phi i64 [ 0, %88 ], [ 0, %91 ], [ 0, %94 ], [ 0, %97 ], [ %103, %99 ], [ 0, %104 ], [ %112, %110 ]
  %127 = zext i64 %126 to i128
  %128 = shl nuw i128 %127, 64
  %129 = zext i64 %125 to i128
  %130 = or i128 %128, %129
  %131 = add nsw i32 %85, -1
  %132 = zext i32 %131 to i128
  %133 = lshr i128 %130, %132
  %134 = icmp ne i128 %133, 0
  %135 = select i1 %83, i1 %134, i1 false
  br i1 %135, label %136, label %144

136:                                              ; preds = %124
  %137 = sub i128 0, %130
  %138 = icmp eq i32 %85, 128
  %139 = zext i32 %85 to i128
  %140 = shl nsw i128 -1, %139
  %141 = xor i128 %140, -1
  %142 = select i1 %138, i128 -1, i128 %141
  %143 = and i128 %142, %137
  br label %144

144:                                              ; preds = %136, %124
  %145 = phi i128 [ %143, %136 ], [ %130, %124 ]
  call void @llvm.lifetime.start.p0(i64 40, ptr nonnull %5) #14
  %146 = icmp ult i128 %145, 18446744073709551616
  %147 = trunc i128 %145 to i64
  br i1 %146, label %170, label %148

148:                                              ; preds = %144, %148
  %149 = phi i64 [ %164, %148 ], [ 0, %144 ]
  %150 = phi i128 [ %152, %148 ], [ %145, %144 ]
  %151 = freeze i128 %150
  %152 = udiv i128 %151, 100
  %153 = mul i128 %152, 100
  %154 = sub i128 %151, %153
  %155 = trunc i128 %154 to i32
  %156 = urem i32 %155, 10
  %157 = trunc i32 %156 to i8
  %158 = or i8 %157, 48
  %159 = or i64 %149, 1
  %160 = getelementptr inbounds [40 x i8], ptr %5, i64 0, i64 %149
  store i8 %158, ptr %160, align 2, !tbaa !26
  %161 = udiv i32 %155, 10
  %162 = trunc i32 %161 to i8
  %163 = or i8 %162, 48
  %164 = add nuw nsw i64 %149, 2
  %165 = getelementptr inbounds [40 x i8], ptr %5, i64 0, i64 %159
  store i8 %163, ptr %165, align 1, !tbaa !26
  %166 = icmp ult i128 %150, 1844674407370955161600
  br i1 %166, label %167, label %148
167:                                              ; preds = %148
  %168 = trunc i128 %152 to i64
  %169 = trunc i64 %164 to i32
  br label %170

170:                                              ; preds = %167, %144
  %171 = phi i32 [ 0, %144 ], [ %169, %167 ]
  %172 = phi i64 [ %147, %144 ], [ %168, %167 ]
  %173 = icmp ugt i64 %172, 99
  br i1 %173, label %174, label %195

174:                                              ; preds = %170
  %175 = zext i32 %171 to i64
  br label %176

176:                                              ; preds = %174, %176
  %177 = phi i64 [ %175, %174 ], [ %189, %176 ]
  %178 = phi i64 [ %172, %174 ], [ %191, %176 ]
  %179 = urem i64 %178, 100
  %180 = trunc i64 %179 to i32
  %181 = urem i32 %180, 10
  %182 = trunc i32 %181 to i8
  %183 = or i8 %182, 48
  %184 = or i64 %177, 1
  %185 = getelementptr inbounds [40 x i8], ptr %5, i64 0, i64 %177
  store i8 %183, ptr %185, align 1, !tbaa !26
  %186 = udiv i32 %180, 10
  %187 = trunc i32 %186 to i8
  %188 = or i8 %187, 48
  %189 = add nuw nsw i64 %177, 2
  %190 = getelementptr inbounds [40 x i8], ptr %5, i64 0, i64 %184
  store i8 %188, ptr %190, align 1, !tbaa !26
  %191 = udiv i64 %178, 100
  %192 = icmp ugt i64 %178, 9999
  br i1 %192, label %176, label %193
193:                                              ; preds = %176
  %194 = trunc i64 %189 to i32
  br label %195

195:                                              ; preds = %193, %170
  %196 = phi i64 [ %172, %170 ], [ %191, %193 ]
  %197 = phi i32 [ %171, %170 ], [ %194, %193 ]
  %198 = icmp samesign ugt i64 %196, 9
  br i1 %198, label %199, label %210

199:                                              ; preds = %195
  %200 = trunc i64 %196 to i32
  %201 = urem i32 %200, 10
  %202 = trunc i32 %201 to i8
  %203 = or i8 %202, 48
  %204 = or i32 %197, 1
  %205 = zext i32 %197 to i64
  %206 = getelementptr inbounds [40 x i8], ptr %5, i64 0, i64 %205
  store i8 %203, ptr %206, align 1, !tbaa !26
  %207 = udiv i32 %200, 10
  %208 = trunc i32 %207 to i8
  %209 = add nuw nsw i32 %197, 2
  br label %213

210:                                              ; preds = %195
  %211 = trunc i64 %196 to i8
  %212 = or i32 %197, 1
  br label %213

213:                                              ; preds = %210, %199
  %214 = phi i32 [ %197, %210 ], [ %204, %199 ]
  %215 = phi i8 [ %211, %210 ], [ %208, %199 ]
  %216 = phi i32 [ %212, %210 ], [ %209, %199 ]
  %217 = or i8 %215, 48
  %218 = zext i32 %214 to i64
  %219 = getelementptr inbounds [40 x i8], ptr %5, i64 0, i64 %218
  store i8 %217, ptr %219, align 1, !tbaa !26
  br i1 %135, label %220, label %221

220:                                              ; preds = %213
  store i8 45, ptr %0, align 1, !tbaa !26
  br label %221

221:                                              ; preds = %220, %213
  %222 = phi i32 [ 1, %220 ], [ 0, %213 ]
  %223 = sext i32 %216 to i64
  %224 = zext i32 %222 to i64
  %225 = add i32 %216, %222
  %226 = zext i32 %225 to i64
  br label %227

227:                                              ; preds = %221, %227
  %228 = phi i64 [ %224, %221 ], [ %233, %227 ]
  %229 = phi i64 [ %223, %221 ], [ %230, %227 ]
  %230 = add nsw i64 %229, -1
  %231 = getelementptr inbounds [40 x i8], ptr %5, i64 0, i64 %230
  %232 = load i8, ptr %231, align 1, !tbaa !26
  %233 = add nuw nsw i64 %228, 1
  %234 = getelementptr inbounds i8, ptr %0, i64 %228
  store i8 %232, ptr %234, align 1, !tbaa !26
  %235 = icmp eq i64 %233, %226
  br i1 %235, label %236, label %227
236:                                              ; preds = %227
  %237 = trunc i64 %233 to i32
  call void @llvm.lifetime.end.p0(i64 40, ptr nonnull %5) #14
  br label %301

238:                                              ; preds = %82
  br i1 %86, label %239, label %299

239:                                              ; preds = %238
  switch i32 %85, label %256 [
    i32 8, label %240
    i32 16, label %243
    i32 32, label %246
    i32 64, label %249
    i32 128, label %251
  ]

240:                                              ; preds = %239
  %241 = load i8, ptr %1, align 1, !tbaa !26
  %242 = zext i8 %241 to i64
  br label %276

243:                                              ; preds = %239
  %244 = load i16, ptr %1, align 1
  %245 = zext i16 %244 to i64
  br label %276

246:                                              ; preds = %239
  %247 = load i32, ptr %1, align 1
  %248 = zext i32 %247 to i64
  br label %276

249:                                              ; preds = %239
  %250 = load i64, ptr %1, align 1
  br label %276

251:                                              ; preds = %239
  %252 = load i128, ptr %1, align 1
  %253 = trunc i128 %252 to i64
  %254 = lshr i128 %252, 64
  %255 = trunc i128 %254 to i64
  br label %276

256:                                              ; preds = %239
  %257 = or i32 %85, 7
  %258 = icmp samesign ult i32 %257, 15
  br i1 %258, label %276, label %259

259:                                              ; preds = %256
  %260 = lshr exact i32 %85, 3
  %261 = zext i32 %260 to i64
  br label %266

262:                                              ; preds = %266
  %263 = lshr i128 %270, 64
  %264 = trunc i128 %263 to i64
  %265 = trunc i128 %274 to i64
  br label %276

266:                                              ; preds = %266, %259
  %267 = phi i64 [ %261, %259 ], [ %269, %266 ]
  %268 = phi i128 [ 0, %259 ], [ %274, %266 ]
  %269 = add nsw i64 %267, -1
  %270 = shl i128 %268, 8
  %271 = getelementptr inbounds i8, ptr %1, i64 %269
  %272 = load i8, ptr %271, align 1, !tbaa !26
  %273 = zext i8 %272 to i128
  %274 = or i128 %270, %273
  %275 = icmp eq i64 %269, 0
  br i1 %275, label %262, label %266
276:                                              ; preds = %240, %243, %246, %249, %251, %256, %262
  %277 = phi i64 [ %242, %240 ], [ %245, %243 ], [ %248, %246 ], [ %250, %249 ], [ %253, %251 ], [ 0, %256 ], [ %265, %262 ]
  %278 = phi i64 [ 0, %240 ], [ 0, %243 ], [ 0, %246 ], [ 0, %249 ], [ %255, %251 ], [ 0, %256 ], [ %264, %262 ]
  %279 = zext i64 %278 to i128
  %280 = shl nuw i128 %279, 64
  %281 = zext i64 %277 to i128
  %282 = or i128 %280, %281
  %283 = add nsw i32 %85, -1
  %284 = zext i32 %283 to i128
  %285 = shl nsw i128 -1, %284
  %286 = xor i128 %285, -1
  %287 = and i128 %282, %286
  %288 = icmp eq i128 %287, 0
  br i1 %288, label %289, label %297

289:                                              ; preds = %276
  %290 = icmp eq i128 %282, 0
  br i1 %290, label %292, label %291

291:                                              ; preds = %289
  store i8 45, ptr %0, align 1, !tbaa !26
  br label %292

292:                                              ; preds = %291, %289
  %293 = phi i32 [ 1, %291 ], [ 0, %289 ]
  %294 = add nuw nsw i32 %293, 1
  %295 = zext i32 %293 to i64
  %296 = getelementptr inbounds i8, ptr %0, i64 %295
  store i8 48, ptr %296, align 1, !tbaa !26
  br label %297

297:                                              ; preds = %276, %292
  %298 = phi i32 [ %294, %292 ], [ undef, %276 ]
  br i1 %288, label %301, label %299

299:                                              ; preds = %297, %238
  %300 = tail call fastcc i32 @format_float(ptr noundef %0, ptr noundef %1, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %4) #15
  br label %301

301:                                              ; preds = %297, %299, %236
  %302 = phi i32 [ %237, %236 ], [ %300, %299 ], [ %298, %297 ]
  call void @llvm.lifetime.end.p0(i64 36, ptr nonnull %4) #14
  ret i32 %302
}

; Function Attrs: noinline nounwind memory(argmem: readwrite, inaccessiblemem: write)
define internal fastcc i32 @format_float(ptr noundef writeonly %0, ptr noundef readonly %1, ptr noundef readonly byval(%struct.tzrt_format) align 8 %2) unnamed_addr #5 {
  %4 = alloca [12 x i8], align 1
  %5 = alloca %struct.tzrt_big, align 4
  %6 = alloca %struct.tzrt_big, align 4
  %7 = alloca %struct.tzrt_big, align 4
  %8 = alloca %struct.tzrt_big, align 4
  %9 = alloca %struct.tzrt_big, align 4
  %10 = alloca %struct.tzrt_big, align 4
  %11 = alloca %struct.tzrt_number, align 4
  %12 = alloca [48 x i8], align 16
  call void @llvm.lifetime.start.p0(i64 5616, ptr nonnull %11) #14
  call fastcc void @decode(ptr sret(%struct.tzrt_number) align 4 %11, ptr noundef %1, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %2) #15
  %13 = getelementptr inbounds i8, ptr %11, i64 5608
  %14 = load i32, ptr %13, align 4, !tbaa !20
  %15 = icmp ne i32 %14, 0
  %16 = getelementptr inbounds i8, ptr %11, i64 5612
  %17 = load i32, ptr %16, align 4
  %18 = icmp ne i32 %17, 2
  %19 = select i1 %15, i1 %18, i1 false
  br i1 %19, label %20, label %21

20:                                               ; preds = %3
  store i8 45, ptr %0, align 1, !tbaa !26
  br label %21

21:                                               ; preds = %20, %3
  %22 = phi i32 [ 1, %20 ], [ 0, %3 ]
  %23 = icmp eq i32 %17, 0
  br i1 %23, label %37, label %24

24:                                               ; preds = %21
  %25 = icmp eq i32 %17, 2
  %26 = select i1 %25, ptr @.str, ptr @.str.1
  %27 = zext i32 %22 to i64
  br label %28

28:                                               ; preds = %24, %28
  %29 = phi i64 [ 0, %24 ], [ %35, %28 ]
  %30 = phi i64 [ %27, %24 ], [ %33, %28 ]
  %31 = getelementptr inbounds i8, ptr %26, i64 %29
  %32 = load i8, ptr %31, align 1, !tbaa !26
  %33 = add nuw nsw i64 %30, 1
  %34 = getelementptr inbounds i8, ptr %0, i64 %30
  store i8 %32, ptr %34, align 1, !tbaa !26
  %35 = add nuw nsw i64 %29, 1
  %36 = icmp eq i64 %35, 3
  br i1 %36, label %1121, label %28
37:                                               ; preds = %21
  %38 = load i32, ptr %11, align 4, !tbaa !25
  %39 = icmp eq i32 %38, 0
  br i1 %39, label %40, label %44

40:                                               ; preds = %37
  %41 = add nuw nsw i32 %22, 1
  %42 = zext i32 %22 to i64
  %43 = getelementptr inbounds i8, ptr %0, i64 %42
  store i8 48, ptr %43, align 1, !tbaa !26
  br label %1123

44:                                               ; preds = %37
  %45 = load i32, ptr %2, align 8, !tbaa !4
  %46 = icmp eq i32 %45, 2
  br i1 %46, label %47, label %904

47:                                               ; preds = %44
  %48 = getelementptr inbounds i8, ptr %2, i64 8
  %49 = load i32, ptr %48, align 8
  %50 = getelementptr inbounds i8, ptr %2, i64 16
  %51 = load i32, ptr %50, align 8
  %52 = icmp sgt i32 %38, 4
  br i1 %52, label %56, label %53

53:                                               ; preds = %47
  %54 = getelementptr inbounds i8, ptr %11, i64 4
  %55 = sext i32 %38 to i64
  br label %62

56:                                               ; preds = %47
  tail call void @llvm.trap()
  unreachable

57:                                               ; preds = %62
  %58 = and i128 %69, 1
  %59 = icmp eq i128 %58, 0
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %5) #14
  %60 = shl i128 %70, 2
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %5, i8 0, i64 5604, i1 false), !alias.scope !106
  %61 = icmp eq i128 %60, 0
  br i1 %61, label %83, label %72

62:                                               ; preds = %62, %53
  %63 = phi i64 [ %55, %53 ], [ %65, %62 ]
  %64 = phi i128 [ 0, %53 ], [ %70, %62 ]
  %65 = add nsw i64 %63, -1
  %66 = shl i128 %64, 32
  %67 = getelementptr inbounds [1400 x i32], ptr %54, i64 0, i64 %65
  %68 = load i32, ptr %67, align 4, !tbaa !24
  %69 = zext i32 %68 to i128
  %70 = or i128 %66, %69
  %71 = icmp eq i64 %65, 0
  br i1 %71, label %57, label %62
72:                                               ; preds = %57
  %73 = getelementptr inbounds i8, ptr %5, i64 4
  br label %74

74:                                               ; preds = %74, %72
  %75 = phi i128 [ %60, %72 ], [ %81, %74 ]
  %76 = trunc i128 %75 to i32
  %77 = load i32, ptr %5, align 4, !tbaa !34, !alias.scope !106
  %78 = add nsw i32 %77, 1
  store i32 %78, ptr %5, align 4, !tbaa !34, !alias.scope !106
  %79 = sext i32 %77 to i64
  %80 = getelementptr inbounds [1400 x i32], ptr %73, i64 0, i64 %79
  store i32 %76, ptr %80, align 4, !tbaa !24, !alias.scope !106
  %81 = lshr i128 %75, 32
  %82 = icmp ult i128 %75, 4294967296
  br i1 %82, label %83, label %74
83:                                               ; preds = %74, %57
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %6) #14
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %6, i8 0, i64 5604, i1 false), !alias.scope !109
  %84 = getelementptr inbounds i8, ptr %6, i64 4
  store i32 1, ptr %6, align 4, !tbaa !34, !alias.scope !109
  store i32 4, ptr %84, align 4, !tbaa !24, !alias.scope !109
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %7) #14
  %85 = zext i32 %51 to i128
  %86 = shl nuw i128 1, %85
  %87 = icmp eq i128 %70, %86
  %88 = getelementptr inbounds i8, ptr %11, i64 5604
  %89 = load i32, ptr %88, align 4
  %90 = icmp sgt i32 %89, %49
  %91 = select i1 %87, i1 %90, i1 false
  %92 = select i1 %91, i32 1, i32 2
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %7, i8 0, i64 5604, i1 false), !alias.scope !112
  %93 = getelementptr inbounds i8, ptr %7, i64 4
  store i32 1, ptr %7, align 4, !tbaa !34, !alias.scope !112
  store i32 %92, ptr %93, align 4, !tbaa !24, !alias.scope !112
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %8) #14
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %8, i8 0, i64 5604, i1 false), !alias.scope !115
  %94 = getelementptr inbounds i8, ptr %8, i64 4
  store i32 1, ptr %8, align 4, !tbaa !34, !alias.scope !115
  store i32 2, ptr %94, align 4, !tbaa !24, !alias.scope !115
  %95 = load i32, ptr %88, align 4, !tbaa !35
  %96 = icmp sgt i32 %95, -1
  br i1 %96, label %97, label %227

97:                                               ; preds = %83
  %98 = load i32, ptr %5, align 4, !tbaa !34
  %99 = icmp ne i32 %98, 0
  %100 = icmp ne i32 %95, 0
  %101 = and i1 %100, %99
  br i1 %101, label %102, label %154

102:                                              ; preds = %97
  %103 = lshr i32 %95, 5
  %104 = and i32 %95, 31
  %105 = add nsw i32 %98, %103
  %106 = icmp sgt i32 %105, 1399
  br i1 %106, label %111, label %107

107:                                              ; preds = %102
  %108 = getelementptr inbounds i8, ptr %5, i64 4
  %109 = sext i32 %98 to i64
  %110 = zext i32 %103 to i64
  br label %114

111:                                              ; preds = %102
  tail call void @llvm.trap()
  unreachable

112:                                              ; preds = %114
  %113 = icmp sgt i32 %95, 31
  br i1 %113, label %129, label %122

114:                                              ; preds = %114, %107
  %115 = phi i64 [ %109, %107 ], [ %116, %114 ]
  %116 = add nsw i64 %115, -1
  %117 = getelementptr inbounds [1400 x i32], ptr %108, i64 0, i64 %116
  %118 = load i32, ptr %117, align 4, !tbaa !24
  %119 = add nsw i64 %116, %110
  %120 = getelementptr inbounds [1400 x i32], ptr %108, i64 0, i64 %119
  store i32 %118, ptr %120, align 4, !tbaa !24
  %121 = icmp eq i64 %116, 0
  br i1 %121, label %112, label %114
122:                                              ; preds = %129, %112
  %123 = load i32, ptr %5, align 4, !tbaa !34
  %124 = add nsw i32 %123, %103
  store i32 %124, ptr %5, align 4, !tbaa !34
  %125 = icmp sgt i32 %123, 0
  br i1 %125, label %126, label %134

126:                                              ; preds = %122
  %127 = zext i32 %104 to i64
  %128 = zext i32 %124 to i64
  br label %137

129:                                              ; preds = %112, %129
  %130 = phi i64 [ %132, %129 ], [ 0, %112 ]
  %131 = getelementptr inbounds [1400 x i32], ptr %108, i64 0, i64 %130
  store i32 0, ptr %131, align 4, !tbaa !24
  %132 = add nuw nsw i64 %130, 1
  %133 = icmp eq i64 %132, %110
  br i1 %133, label %122, label %129
134:                                              ; preds = %137, %122
  %135 = phi i64 [ 0, %122 ], [ %146, %137 ]
  %136 = icmp eq i64 %135, 0
  br i1 %136, label %154, label %149

137:                                              ; preds = %137, %126
  %138 = phi i64 [ %110, %126 ], [ %147, %137 ]
  %139 = phi i64 [ 0, %126 ], [ %146, %137 ]
  %140 = getelementptr inbounds [1400 x i32], ptr %108, i64 0, i64 %138
  %141 = load i32, ptr %140, align 4, !tbaa !24
  %142 = zext i32 %141 to i64
  %143 = shl nuw nsw i64 %142, %127
  %144 = or i64 %143, %139
  %145 = trunc i64 %144 to i32
  store i32 %145, ptr %140, align 4, !tbaa !24
  %146 = lshr i64 %143, 32
  %147 = add nuw nsw i64 %138, 1
  %148 = icmp samesign ult i64 %147, %128
  br i1 %148, label %137, label %134
149:                                              ; preds = %134
  %150 = trunc i64 %135 to i32
  %151 = add nsw i32 %124, 1
  store i32 %151, ptr %5, align 4, !tbaa !34
  %152 = sext i32 %124 to i64
  %153 = getelementptr inbounds [1400 x i32], ptr %108, i64 0, i64 %152
  store i32 %150, ptr %153, align 4, !tbaa !24
  br label %154

154:                                              ; preds = %149, %134, %97
  br i1 %100, label %155, label %274

155:                                              ; preds = %154
  %156 = lshr i32 %95, 5
  %157 = and i32 %95, 31
  %158 = icmp ugt i32 %95, 44767
  br i1 %158, label %164, label %159

159:                                              ; preds = %155
  %160 = zext i32 %156 to i64
  %161 = load i32, ptr %93, align 4, !tbaa !24
  %162 = getelementptr inbounds [1400 x i32], ptr %93, i64 0, i64 %160
  store i32 %161, ptr %162, align 4, !tbaa !24
  %163 = icmp sgt i32 %95, 31
  br i1 %163, label %172, label %165

164:                                              ; preds = %155
  tail call void @llvm.trap()
  unreachable

165:                                              ; preds = %172, %159
  %166 = load i32, ptr %7, align 4, !tbaa !34
  %167 = add nsw i32 %166, %156
  store i32 %167, ptr %7, align 4, !tbaa !34
  %168 = icmp sgt i32 %166, 0
  br i1 %168, label %169, label %177

169:                                              ; preds = %165
  %170 = zext i32 %157 to i64
  %171 = zext i32 %167 to i64
  br label %180

172:                                              ; preds = %159, %172
  %173 = phi i64 [ %175, %172 ], [ 0, %159 ]
  %174 = getelementptr inbounds [1400 x i32], ptr %93, i64 0, i64 %173
  store i32 0, ptr %174, align 4, !tbaa !24
  %175 = add nuw nsw i64 %173, 1
  %176 = icmp eq i64 %175, %160
  br i1 %176, label %165, label %172
177:                                              ; preds = %180, %165
  %178 = phi i64 [ 0, %165 ], [ %189, %180 ]
  %179 = icmp eq i64 %178, 0
  br i1 %179, label %197, label %192

180:                                              ; preds = %180, %169
  %181 = phi i64 [ %160, %169 ], [ %190, %180 ]
  %182 = phi i64 [ 0, %169 ], [ %189, %180 ]
  %183 = getelementptr inbounds [1400 x i32], ptr %93, i64 0, i64 %181
  %184 = load i32, ptr %183, align 4, !tbaa !24
  %185 = zext i32 %184 to i64
  %186 = shl nuw nsw i64 %185, %170
  %187 = or i64 %186, %182
  %188 = trunc i64 %187 to i32
  store i32 %188, ptr %183, align 4, !tbaa !24
  %189 = lshr i64 %186, 32
  %190 = add nuw nsw i64 %181, 1
  %191 = icmp samesign ult i64 %190, %171
  br i1 %191, label %180, label %177
192:                                              ; preds = %177
  %193 = trunc i64 %178 to i32
  %194 = add nsw i32 %167, 1
  store i32 %194, ptr %7, align 4, !tbaa !34
  %195 = sext i32 %167 to i64
  %196 = getelementptr inbounds [1400 x i32], ptr %93, i64 0, i64 %195
  store i32 %193, ptr %196, align 4, !tbaa !24
  br label %197

197:                                              ; preds = %177, %192
  %198 = load i32, ptr %94, align 4, !tbaa !24
  %199 = getelementptr inbounds [1400 x i32], ptr %94, i64 0, i64 %160
  store i32 %198, ptr %199, align 4, !tbaa !24
  br i1 %163, label %207, label %200

200:                                              ; preds = %207, %197
  %201 = load i32, ptr %8, align 4, !tbaa !34
  %202 = add nsw i32 %201, %156
  store i32 %202, ptr %8, align 4, !tbaa !34
  %203 = icmp sgt i32 %201, 0
  br i1 %203, label %204, label %212

204:                                              ; preds = %200
  %205 = zext i32 %157 to i64
  %206 = zext i32 %202 to i64
  br label %215

207:                                              ; preds = %197, %207
  %208 = phi i64 [ %210, %207 ], [ 0, %197 ]
  %209 = getelementptr inbounds [1400 x i32], ptr %94, i64 0, i64 %208
  store i32 0, ptr %209, align 4, !tbaa !24
  %210 = add nuw nsw i64 %208, 1
  %211 = icmp eq i64 %210, %160
  br i1 %211, label %200, label %207
212:                                              ; preds = %215, %200
  %213 = phi i64 [ 0, %200 ], [ %224, %215 ]
  %214 = icmp eq i64 %213, 0
  br i1 %214, label %274, label %265

215:                                              ; preds = %215, %204
  %216 = phi i64 [ %160, %204 ], [ %225, %215 ]
  %217 = phi i64 [ 0, %204 ], [ %224, %215 ]
  %218 = getelementptr inbounds [1400 x i32], ptr %94, i64 0, i64 %216
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
227:                                              ; preds = %83
  %228 = sub nsw i32 0, %95
  %229 = sdiv i32 %95, -32
  %230 = and i32 %228, 31
  %231 = icmp slt i32 %95, -44767
  br i1 %231, label %237, label %232

232:                                              ; preds = %227
  %233 = zext i32 %229 to i64
  %234 = load i32, ptr %84, align 4, !tbaa !24
  %235 = getelementptr inbounds [1400 x i32], ptr %84, i64 0, i64 %233
  store i32 %234, ptr %235, align 4, !tbaa !24
  %236 = icmp slt i32 %95, -31
  br i1 %236, label %245, label %238

237:                                              ; preds = %227
  tail call void @llvm.trap()
  unreachable

238:                                              ; preds = %245, %232
  %239 = load i32, ptr %6, align 4, !tbaa !34
  %240 = add nsw i32 %239, %229
  store i32 %240, ptr %6, align 4, !tbaa !34
  %241 = icmp sgt i32 %239, 0
  br i1 %241, label %242, label %250

242:                                              ; preds = %238
  %243 = zext i32 %230 to i64
  %244 = zext i32 %240 to i64
  br label %253

245:                                              ; preds = %232, %245
  %246 = phi i64 [ %248, %245 ], [ 0, %232 ]
  %247 = getelementptr inbounds [1400 x i32], ptr %84, i64 0, i64 %246
  store i32 0, ptr %247, align 4, !tbaa !24
  %248 = add nuw nsw i64 %246, 1
  %249 = icmp eq i64 %248, %233
  br i1 %249, label %238, label %245
250:                                              ; preds = %253, %238
  %251 = phi i64 [ 0, %238 ], [ %262, %253 ]
  %252 = icmp eq i64 %251, 0
  br i1 %252, label %274, label %265

253:                                              ; preds = %253, %242
  %254 = phi i64 [ %233, %242 ], [ %263, %253 ]
  %255 = phi i64 [ 0, %242 ], [ %262, %253 ]
  %256 = getelementptr inbounds [1400 x i32], ptr %84, i64 0, i64 %254
  %257 = load i32, ptr %256, align 4, !tbaa !24
  %258 = zext i32 %257 to i64
  %259 = shl nuw nsw i64 %258, %243
  %260 = or i64 %259, %255
  %261 = trunc i64 %260 to i32
  store i32 %261, ptr %256, align 4, !tbaa !24
  %262 = lshr i64 %259, 32
  %263 = add nuw nsw i64 %254, 1
  %264 = icmp samesign ult i64 %263, %244
  br i1 %264, label %253, label %250
265:                                              ; preds = %250, %212
  %266 = phi i64 [ %213, %212 ], [ %251, %250 ]
  %267 = phi i32 [ %202, %212 ], [ %240, %250 ]
  %268 = phi ptr [ %8, %212 ], [ %6, %250 ]
  %269 = getelementptr inbounds i8, ptr %268, i64 4
  %270 = trunc i64 %266 to i32
  %271 = add nsw i32 %267, 1
  store i32 %271, ptr %268, align 4, !tbaa !34
  %272 = sext i32 %267 to i64
  %273 = getelementptr inbounds [1400 x i32], ptr %269, i64 0, i64 %272
  store i32 %270, ptr %273, align 4, !tbaa !24
  br label %274

274:                                              ; preds = %265, %154, %250, %212
  %275 = call fastcc i32 @magnitude(ptr noundef %5, ptr noundef %6, i32 noundef 10) #15
  %276 = icmp sgt i32 %275, -1
  br i1 %276, label %277, label %346

277:                                              ; preds = %274
  %278 = icmp sgt i32 %275, 8
  br i1 %278, label %282, label %279

279:                                              ; preds = %311, %277
  %280 = phi i32 [ %275, %277 ], [ %312, %311 ]
  %281 = icmp sgt i32 %280, 0
  br i1 %281, label %314, label %556

282:                                              ; preds = %277, %311
  %283 = phi i32 [ %312, %311 ], [ %275, %277 ]
  %284 = load i32, ptr %6, align 4, !tbaa !34
  %285 = icmp sgt i32 %284, 0
  br i1 %285, label %286, label %288

286:                                              ; preds = %282
  %287 = zext i32 %284 to i64
  br label %291

288:                                              ; preds = %291, %282
  %289 = phi i64 [ 0, %282 ], [ %300, %291 ]
  %290 = icmp eq i64 %289, 0
  br i1 %290, label %311, label %303

291:                                              ; preds = %291, %286
  %292 = phi i64 [ 0, %286 ], [ %301, %291 ]
  %293 = phi i64 [ 0, %286 ], [ %300, %291 ]
  %294 = getelementptr inbounds [1400 x i32], ptr %84, i64 0, i64 %292
  %295 = load i32, ptr %294, align 4, !tbaa !24
  %296 = zext i32 %295 to i64
  %297 = mul nuw nsw i64 %296, 1000000000
  %298 = add nuw nsw i64 %297, %293
  %299 = trunc i64 %298 to i32
  store i32 %299, ptr %294, align 4, !tbaa !24
  %300 = lshr i64 %298, 32
  %301 = add nuw nsw i64 %292, 1
  %302 = icmp eq i64 %301, %287
  br i1 %302, label %288, label %291
303:                                              ; preds = %288
  %304 = icmp eq i32 %284, 1400
  br i1 %304, label %305, label %306

305:                                              ; preds = %303
  tail call void @llvm.trap()
  unreachable

306:                                              ; preds = %303
  %307 = trunc i64 %289 to i32
  %308 = add nsw i32 %284, 1
  store i32 %308, ptr %6, align 4, !tbaa !34
  %309 = sext i32 %284 to i64
  %310 = getelementptr inbounds [1400 x i32], ptr %84, i64 0, i64 %309
  store i32 %307, ptr %310, align 4, !tbaa !24
  br label %311

311:                                              ; preds = %306, %288
  %312 = add nsw i32 %283, -9
  %313 = icmp sgt i32 %283, 17
  br i1 %313, label %282, label %279
314:                                              ; preds = %279, %344
  %315 = phi i32 [ %316, %344 ], [ %280, %279 ]
  %316 = add nsw i32 %315, -1
  %317 = load i32, ptr %6, align 4, !tbaa !34
  %318 = icmp sgt i32 %317, 0
  br i1 %318, label %319, label %321

319:                                              ; preds = %314
  %320 = zext i32 %317 to i64
  br label %324

321:                                              ; preds = %324, %314
  %322 = phi i64 [ 0, %314 ], [ %333, %324 ]
  %323 = icmp eq i64 %322, 0
  br i1 %323, label %344, label %336

324:                                              ; preds = %324, %319
  %325 = phi i64 [ 0, %319 ], [ %334, %324 ]
  %326 = phi i64 [ 0, %319 ], [ %333, %324 ]
  %327 = getelementptr inbounds [1400 x i32], ptr %84, i64 0, i64 %325
  %328 = load i32, ptr %327, align 4, !tbaa !24
  %329 = zext i32 %328 to i64
  %330 = mul nuw nsw i64 %329, 10
  %331 = add nuw nsw i64 %330, %326
  %332 = trunc i64 %331 to i32
  store i32 %332, ptr %327, align 4, !tbaa !24
  %333 = lshr i64 %331, 32
  %334 = add nuw nsw i64 %325, 1
  %335 = icmp eq i64 %334, %320
  br i1 %335, label %321, label %324
336:                                              ; preds = %321
  %337 = icmp eq i32 %317, 1400
  br i1 %337, label %338, label %339

338:                                              ; preds = %336
  tail call void @llvm.trap()
  unreachable

339:                                              ; preds = %336
  %340 = trunc i64 %322 to i32
  %341 = add nsw i32 %317, 1
  store i32 %341, ptr %6, align 4, !tbaa !34
  %342 = sext i32 %317 to i64
  %343 = getelementptr inbounds [1400 x i32], ptr %84, i64 0, i64 %342
  store i32 %340, ptr %343, align 4, !tbaa !24
  br label %344

344:                                              ; preds = %339, %321
  %345 = icmp sgt i32 %315, 1
  br i1 %345, label %314, label %556
346:                                              ; preds = %274
  %347 = sub nsw i32 0, %275
  %348 = icmp slt i32 %275, -8
  br i1 %348, label %349, label %351

349:                                              ; preds = %346
  %350 = getelementptr inbounds i8, ptr %5, i64 4
  br label %356

351:                                              ; preds = %385, %346
  %352 = phi i32 [ %347, %346 ], [ %386, %385 ]
  %353 = icmp sgt i32 %352, 0
  br i1 %353, label %354, label %420

354:                                              ; preds = %351
  %355 = getelementptr inbounds i8, ptr %5, i64 4
  br label %388

356:                                              ; preds = %385, %349
  %357 = phi i32 [ %347, %349 ], [ %386, %385 ]
  %358 = load i32, ptr %5, align 4, !tbaa !34
  %359 = icmp sgt i32 %358, 0
  br i1 %359, label %360, label %362

360:                                              ; preds = %356
  %361 = zext i32 %358 to i64
  br label %365

362:                                              ; preds = %365, %356
  %363 = phi i64 [ 0, %356 ], [ %374, %365 ]
  %364 = icmp eq i64 %363, 0
  br i1 %364, label %385, label %377

365:                                              ; preds = %365, %360
  %366 = phi i64 [ 0, %360 ], [ %375, %365 ]
  %367 = phi i64 [ 0, %360 ], [ %374, %365 ]
  %368 = getelementptr inbounds [1400 x i32], ptr %350, i64 0, i64 %366
  %369 = load i32, ptr %368, align 4, !tbaa !24
  %370 = zext i32 %369 to i64
  %371 = mul nuw nsw i64 %370, 1000000000
  %372 = add nuw nsw i64 %371, %367
  %373 = trunc i64 %372 to i32
  store i32 %373, ptr %368, align 4, !tbaa !24
  %374 = lshr i64 %372, 32
  %375 = add nuw nsw i64 %366, 1
  %376 = icmp eq i64 %375, %361
  br i1 %376, label %362, label %365
377:                                              ; preds = %362
  %378 = icmp eq i32 %358, 1400
  br i1 %378, label %379, label %380

379:                                              ; preds = %377
  tail call void @llvm.trap()
  unreachable

380:                                              ; preds = %377
  %381 = trunc i64 %363 to i32
  %382 = add nsw i32 %358, 1
  store i32 %382, ptr %5, align 4, !tbaa !34
  %383 = sext i32 %358 to i64
  %384 = getelementptr inbounds [1400 x i32], ptr %350, i64 0, i64 %383
  store i32 %381, ptr %384, align 4, !tbaa !24
  br label %385

385:                                              ; preds = %380, %362
  %386 = add nsw i32 %357, -9
  %387 = icmp sgt i32 %357, 17
  br i1 %387, label %356, label %351
388:                                              ; preds = %418, %354
  %389 = phi i32 [ %352, %354 ], [ %390, %418 ]
  %390 = add nsw i32 %389, -1
  %391 = load i32, ptr %5, align 4, !tbaa !34
  %392 = icmp sgt i32 %391, 0
  br i1 %392, label %393, label %395

393:                                              ; preds = %388
  %394 = zext i32 %391 to i64
  br label %398

395:                                              ; preds = %398, %388
  %396 = phi i64 [ 0, %388 ], [ %407, %398 ]
  %397 = icmp eq i64 %396, 0
  br i1 %397, label %418, label %410

398:                                              ; preds = %398, %393
  %399 = phi i64 [ 0, %393 ], [ %408, %398 ]
  %400 = phi i64 [ 0, %393 ], [ %407, %398 ]
  %401 = getelementptr inbounds [1400 x i32], ptr %355, i64 0, i64 %399
  %402 = load i32, ptr %401, align 4, !tbaa !24
  %403 = zext i32 %402 to i64
  %404 = mul nuw nsw i64 %403, 10
  %405 = add nuw nsw i64 %404, %400
  %406 = trunc i64 %405 to i32
  store i32 %406, ptr %401, align 4, !tbaa !24
  %407 = lshr i64 %405, 32
  %408 = add nuw nsw i64 %399, 1
  %409 = icmp eq i64 %408, %394
  br i1 %409, label %395, label %398
410:                                              ; preds = %395
  %411 = icmp eq i32 %391, 1400
  br i1 %411, label %412, label %413

412:                                              ; preds = %410
  tail call void @llvm.trap()
  unreachable

413:                                              ; preds = %410
  %414 = trunc i64 %396 to i32
  %415 = add nsw i32 %391, 1
  store i32 %415, ptr %5, align 4, !tbaa !34
  %416 = sext i32 %391 to i64
  %417 = getelementptr inbounds [1400 x i32], ptr %355, i64 0, i64 %416
  store i32 %414, ptr %417, align 4, !tbaa !24
  br label %418

418:                                              ; preds = %413, %395
  %419 = icmp sgt i32 %389, 1
  br i1 %419, label %388, label %420
420:                                              ; preds = %418, %351
  br i1 %348, label %424, label %421

421:                                              ; preds = %453, %420
  %422 = phi i32 [ %347, %420 ], [ %454, %453 ]
  %423 = icmp sgt i32 %422, 0
  br i1 %423, label %456, label %488

424:                                              ; preds = %420, %453
  %425 = phi i32 [ %454, %453 ], [ %347, %420 ]
  %426 = load i32, ptr %7, align 4, !tbaa !34
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
  %436 = getelementptr inbounds [1400 x i32], ptr %93, i64 0, i64 %434
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
  store i32 %450, ptr %7, align 4, !tbaa !34
  %451 = sext i32 %426 to i64
  %452 = getelementptr inbounds [1400 x i32], ptr %93, i64 0, i64 %451
  store i32 %449, ptr %452, align 4, !tbaa !24
  br label %453

453:                                              ; preds = %448, %430
  %454 = add nsw i32 %425, -9
  %455 = icmp sgt i32 %425, 17
  br i1 %455, label %424, label %421
456:                                              ; preds = %421, %486
  %457 = phi i32 [ %458, %486 ], [ %422, %421 ]
  %458 = add nsw i32 %457, -1
  %459 = load i32, ptr %7, align 4, !tbaa !34
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
  %469 = getelementptr inbounds [1400 x i32], ptr %93, i64 0, i64 %467
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
  store i32 %483, ptr %7, align 4, !tbaa !34
  %484 = sext i32 %459 to i64
  %485 = getelementptr inbounds [1400 x i32], ptr %93, i64 0, i64 %484
  store i32 %482, ptr %485, align 4, !tbaa !24
  br label %486

486:                                              ; preds = %481, %463
  %487 = icmp sgt i32 %457, 1
  br i1 %487, label %456, label %488
488:                                              ; preds = %486, %421
  br i1 %348, label %492, label %489

489:                                              ; preds = %521, %488
  %490 = phi i32 [ %347, %488 ], [ %522, %521 ]
  %491 = icmp sgt i32 %490, 0
  br i1 %491, label %524, label %556

492:                                              ; preds = %488, %521
  %493 = phi i32 [ %522, %521 ], [ %347, %488 ]
  %494 = load i32, ptr %8, align 4, !tbaa !34
  %495 = icmp sgt i32 %494, 0
  br i1 %495, label %496, label %498

496:                                              ; preds = %492
  %497 = zext i32 %494 to i64
  br label %501

498:                                              ; preds = %501, %492
  %499 = phi i64 [ 0, %492 ], [ %510, %501 ]
  %500 = icmp eq i64 %499, 0
  br i1 %500, label %521, label %513

501:                                              ; preds = %501, %496
  %502 = phi i64 [ 0, %496 ], [ %511, %501 ]
  %503 = phi i64 [ 0, %496 ], [ %510, %501 ]
  %504 = getelementptr inbounds [1400 x i32], ptr %94, i64 0, i64 %502
  %505 = load i32, ptr %504, align 4, !tbaa !24
  %506 = zext i32 %505 to i64
  %507 = mul nuw nsw i64 %506, 1000000000
  %508 = add nuw nsw i64 %507, %503
  %509 = trunc i64 %508 to i32
  store i32 %509, ptr %504, align 4, !tbaa !24
  %510 = lshr i64 %508, 32
  %511 = add nuw nsw i64 %502, 1
  %512 = icmp eq i64 %511, %497
  br i1 %512, label %498, label %501
513:                                              ; preds = %498
  %514 = icmp eq i32 %494, 1400
  br i1 %514, label %515, label %516

515:                                              ; preds = %513
  tail call void @llvm.trap()
  unreachable

516:                                              ; preds = %513
  %517 = trunc i64 %499 to i32
  %518 = add nsw i32 %494, 1
  store i32 %518, ptr %8, align 4, !tbaa !34
  %519 = sext i32 %494 to i64
  %520 = getelementptr inbounds [1400 x i32], ptr %94, i64 0, i64 %519
  store i32 %517, ptr %520, align 4, !tbaa !24
  br label %521

521:                                              ; preds = %516, %498
  %522 = add nsw i32 %493, -9
  %523 = icmp sgt i32 %493, 17
  br i1 %523, label %492, label %489
524:                                              ; preds = %489, %554
  %525 = phi i32 [ %526, %554 ], [ %490, %489 ]
  %526 = add nsw i32 %525, -1
  %527 = load i32, ptr %8, align 4, !tbaa !34
  %528 = icmp sgt i32 %527, 0
  br i1 %528, label %529, label %531

529:                                              ; preds = %524
  %530 = zext i32 %527 to i64
  br label %534

531:                                              ; preds = %534, %524
  %532 = phi i64 [ 0, %524 ], [ %543, %534 ]
  %533 = icmp eq i64 %532, 0
  br i1 %533, label %554, label %546

534:                                              ; preds = %534, %529
  %535 = phi i64 [ 0, %529 ], [ %544, %534 ]
  %536 = phi i64 [ 0, %529 ], [ %543, %534 ]
  %537 = getelementptr inbounds [1400 x i32], ptr %94, i64 0, i64 %535
  %538 = load i32, ptr %537, align 4, !tbaa !24
  %539 = zext i32 %538 to i64
  %540 = mul nuw nsw i64 %539, 10
  %541 = add nuw nsw i64 %540, %536
  %542 = trunc i64 %541 to i32
  store i32 %542, ptr %537, align 4, !tbaa !24
  %543 = lshr i64 %541, 32
  %544 = add nuw nsw i64 %535, 1
  %545 = icmp eq i64 %544, %530
  br i1 %545, label %531, label %534
546:                                              ; preds = %531
  %547 = icmp eq i32 %527, 1400
  br i1 %547, label %548, label %549

548:                                              ; preds = %546
  tail call void @llvm.trap()
  unreachable

549:                                              ; preds = %546
  %550 = trunc i64 %532 to i32
  %551 = add nsw i32 %527, 1
  store i32 %551, ptr %8, align 4, !tbaa !34
  %552 = sext i32 %527 to i64
  %553 = getelementptr inbounds [1400 x i32], ptr %94, i64 0, i64 %552
  store i32 %550, ptr %553, align 4, !tbaa !24
  br label %554

554:                                              ; preds = %549, %531
  %555 = icmp sgt i32 %525, 1
  br i1 %555, label %524, label %556
556:                                              ; preds = %554, %344, %489, %279
  %557 = load i32, ptr %6, align 4, !tbaa !34
  %558 = getelementptr inbounds i8, ptr %5, i64 4
  %559 = sext i32 %557 to i64
  %560 = getelementptr inbounds i8, ptr %9, i64 4
  %561 = getelementptr inbounds i8, ptr %10, i64 4
  %562 = add i32 %275, 1
  %563 = load i32, ptr %88, align 4
  br label %564

564:                                              ; preds = %899, %556
  %565 = phi i32 [ %563, %556 ], [ %900, %899 ]
  %566 = phi i32 [ 1, %556 ], [ %902, %899 ]
  %567 = phi i128 [ 0, %556 ], [ %901, %899 ]
  %568 = load i32, ptr %5, align 4, !tbaa !34
  br label %569

569:                                              ; preds = %635, %564
  %570 = phi i32 [ %568, %564 ], [ %636, %635 ]
  %571 = phi i32 [ 0, %564 ], [ %637, %635 ]
  %572 = icmp eq i32 %570, %557
  br i1 %572, label %573, label %577

573:                                              ; preds = %569
  %574 = icmp eq i32 %570, 0
  br i1 %574, label %593, label %575

575:                                              ; preds = %573
  %576 = sext i32 %570 to i64
  br label %582

577:                                              ; preds = %569
  %578 = icmp slt i32 %570, %557
  %579 = select i1 %578, i32 -1, i32 1
  br label %593

580:                                              ; preds = %582
  %581 = icmp eq i64 %584, 0
  br i1 %581, label %593, label %582
582:                                              ; preds = %575, %580
  %583 = phi i64 [ %576, %575 ], [ %584, %580 ]
  %584 = add nsw i64 %583, -1
  %585 = getelementptr inbounds [1400 x i32], ptr %558, i64 0, i64 %584
  %586 = load i32, ptr %585, align 4, !tbaa !24
  %587 = getelementptr inbounds [1400 x i32], ptr %84, i64 0, i64 %584
  %588 = load i32, ptr %587, align 4, !tbaa !24
  %589 = icmp eq i32 %586, %588
  br i1 %589, label %580, label %590
590:                                              ; preds = %582
  %591 = icmp ult i32 %586, %588
  %592 = select i1 %591, i32 -1, i32 1
  br label %593

593:                                              ; preds = %580, %573, %590, %577
  %594 = phi i32 [ %579, %577 ], [ %592, %590 ], [ 0, %573 ], [ 0, %580 ]
  %595 = icmp sgt i32 %594, -1
  br i1 %595, label %596, label %638

596:                                              ; preds = %593
  %597 = icmp sgt i32 %570, 0
  br i1 %597, label %598, label %600

598:                                              ; preds = %596
  %599 = zext i32 %570 to i64
  br label %613

600:                                              ; preds = %621, %596
  %601 = icmp eq i32 %570, 0
  br i1 %601, label %635, label %602

602:                                              ; preds = %600
  %603 = sext i32 %570 to i64
  br label %604

604:                                              ; preds = %610, %602
  %605 = phi i64 [ %603, %602 ], [ %606, %610 ]
  %606 = add nsw i64 %605, -1
  %607 = getelementptr inbounds [1400 x i32], ptr %558, i64 0, i64 %606
  %608 = load i32, ptr %607, align 4, !tbaa !24
  %609 = icmp eq i32 %608, 0
  br i1 %609, label %610, label %633

610:                                              ; preds = %604
  %611 = trunc i64 %606 to i32
  store i32 %611, ptr %5, align 4, !tbaa !34
  %612 = icmp eq i64 %606, 0
  br i1 %612, label %635, label %604
613:                                              ; preds = %621, %598
  %614 = phi i64 [ 0, %598 ], [ %631, %621 ]
  %615 = phi i64 [ 0, %598 ], [ %630, %621 ]
  %616 = icmp slt i64 %614, %559
  br i1 %616, label %617, label %621

617:                                              ; preds = %613
  %618 = getelementptr inbounds [1400 x i32], ptr %84, i64 0, i64 %614
  %619 = load i32, ptr %618, align 4, !tbaa !24
  %620 = zext i32 %619 to i64
  br label %621

621:                                              ; preds = %617, %613
  %622 = phi i64 [ %620, %617 ], [ 0, %613 ]
  %623 = add nuw nsw i64 %622, %615
  %624 = getelementptr inbounds [1400 x i32], ptr %558, i64 0, i64 %614
  %625 = load i32, ptr %624, align 4, !tbaa !24
  %626 = zext i32 %625 to i64
  %627 = trunc i64 %623 to i32
  %628 = sub i32 %625, %627
  store i32 %628, ptr %624, align 4, !tbaa !24
  %629 = icmp samesign ugt i64 %623, %626
  %630 = zext i1 %629 to i64
  %631 = add nuw nsw i64 %614, 1
  %632 = icmp eq i64 %631, %599
  br i1 %632, label %600, label %613
633:                                              ; preds = %604
  %634 = trunc i64 %605 to i32
  br label %635

635:                                              ; preds = %610, %633, %600
  %636 = phi i32 [ %570, %600 ], [ %634, %633 ], [ 0, %610 ]
  %637 = add i32 %571, 1
  br label %569
638:                                              ; preds = %593
  %639 = mul i128 %567, 10
  %640 = zext i32 %571 to i128
  %641 = add i128 %639, %640
  %642 = load i32, ptr %7, align 4, !tbaa !34
  %643 = icmp eq i32 %570, %642
  br i1 %643, label %644, label %648

644:                                              ; preds = %638
  %645 = icmp eq i32 %570, 0
  br i1 %645, label %664, label %646

646:                                              ; preds = %644
  %647 = sext i32 %570 to i64
  br label %653

648:                                              ; preds = %638
  %649 = icmp slt i32 %570, %642
  %650 = select i1 %649, i32 -1, i32 1
  br label %664

651:                                              ; preds = %653
  %652 = icmp eq i64 %655, 0
  br i1 %652, label %664, label %653
653:                                              ; preds = %646, %651
  %654 = phi i64 [ %647, %646 ], [ %655, %651 ]
  %655 = add nsw i64 %654, -1
  %656 = getelementptr inbounds [1400 x i32], ptr %558, i64 0, i64 %655
  %657 = load i32, ptr %656, align 4, !tbaa !24
  %658 = getelementptr inbounds [1400 x i32], ptr %93, i64 0, i64 %655
  %659 = load i32, ptr %658, align 4, !tbaa !24
  %660 = icmp eq i32 %657, %659
  br i1 %660, label %651, label %661
661:                                              ; preds = %653
  %662 = icmp ult i32 %657, %659
  %663 = select i1 %662, i32 -1, i32 1
  br label %664

664:                                              ; preds = %651, %644, %661, %648
  %665 = phi i32 [ %650, %648 ], [ %663, %661 ], [ 0, %644 ], [ 0, %651 ]
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %9) #14
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %9, ptr noundef nonnull align 4 dereferenceable(5604) %6, i64 5604, i1 false), !tbaa.struct !30
  %666 = load i32, ptr %9, align 4, !tbaa !34
  %667 = icmp sgt i32 %666, 0
  br i1 %667, label %668, label %671

668:                                              ; preds = %664
  %669 = zext i32 %666 to i64
  %670 = sext i32 %570 to i64
  br label %684

671:                                              ; preds = %692, %664
  %672 = icmp eq i32 %666, 0
  br i1 %672, label %704, label %673

673:                                              ; preds = %671
  %674 = sext i32 %666 to i64
  br label %675

675:                                              ; preds = %681, %673
  %676 = phi i64 [ %674, %673 ], [ %677, %681 ]
  %677 = add nsw i64 %676, -1
  %678 = getelementptr inbounds [1400 x i32], ptr %560, i64 0, i64 %677
  %679 = load i32, ptr %678, align 4, !tbaa !24
  %680 = icmp eq i32 %679, 0
  br i1 %680, label %681, label %704

681:                                              ; preds = %675
  %682 = trunc i64 %677 to i32
  store i32 %682, ptr %9, align 4, !tbaa !34
  %683 = icmp eq i64 %677, 0
  br i1 %683, label %704, label %675
684:                                              ; preds = %692, %668
  %685 = phi i64 [ 0, %668 ], [ %702, %692 ]
  %686 = phi i64 [ 0, %668 ], [ %701, %692 ]
  %687 = icmp slt i64 %685, %670
  br i1 %687, label %688, label %692

688:                                              ; preds = %684
  %689 = getelementptr inbounds [1400 x i32], ptr %558, i64 0, i64 %685
  %690 = load i32, ptr %689, align 4, !tbaa !24
  %691 = zext i32 %690 to i64
  br label %692

692:                                              ; preds = %688, %684
  %693 = phi i64 [ %691, %688 ], [ 0, %684 ]
  %694 = add nuw nsw i64 %693, %686
  %695 = getelementptr inbounds [1400 x i32], ptr %560, i64 0, i64 %685
  %696 = load i32, ptr %695, align 4, !tbaa !24
  %697 = zext i32 %696 to i64
  %698 = trunc i64 %694 to i32
  %699 = sub i32 %696, %698
  store i32 %699, ptr %695, align 4, !tbaa !24
  %700 = icmp samesign ugt i64 %694, %697
  %701 = zext i1 %700 to i64
  %702 = add nuw nsw i64 %685, 1
  %703 = icmp eq i64 %702, %669
  br i1 %703, label %671, label %684
704:                                              ; preds = %681, %675, %671
  %705 = load i32, ptr %9, align 4, !tbaa !34
  %706 = load i32, ptr %8, align 4, !tbaa !34
  %707 = icmp eq i32 %705, %706
  br i1 %707, label %708, label %712

708:                                              ; preds = %704
  %709 = icmp eq i32 %705, 0
  br i1 %709, label %728, label %710

710:                                              ; preds = %708
  %711 = sext i32 %705 to i64
  br label %717

712:                                              ; preds = %704
  %713 = icmp slt i32 %705, %706
  %714 = select i1 %713, i32 -1, i32 1
  br label %728

715:                                              ; preds = %717
  %716 = icmp eq i64 %719, 0
  br i1 %716, label %728, label %717
717:                                              ; preds = %710, %715
  %718 = phi i64 [ %711, %710 ], [ %719, %715 ]
  %719 = add nsw i64 %718, -1
  %720 = getelementptr inbounds [1400 x i32], ptr %560, i64 0, i64 %719
  %721 = load i32, ptr %720, align 4, !tbaa !24
  %722 = getelementptr inbounds [1400 x i32], ptr %94, i64 0, i64 %719
  %723 = load i32, ptr %722, align 4, !tbaa !24
  %724 = icmp eq i32 %721, %723
  br i1 %724, label %715, label %725
725:                                              ; preds = %717
  %726 = icmp ult i32 %721, %723
  %727 = select i1 %726, i32 -1, i32 1
  br label %728

728:                                              ; preds = %715, %708, %725, %712
  %729 = phi i32 [ %714, %712 ], [ %727, %725 ], [ 0, %708 ], [ 0, %715 ]
  %730 = icmp slt i32 %665, 0
  br i1 %730, label %734, label %731

731:                                              ; preds = %728
  %732 = icmp eq i32 %665, 0
  %733 = and i1 %59, %732
  br label %734

734:                                              ; preds = %731, %728
  %735 = phi i1 [ true, %728 ], [ %733, %731 ]
  %736 = icmp slt i32 %729, 0
  br i1 %736, label %740, label %737

737:                                              ; preds = %734
  %738 = icmp eq i32 %729, 0
  %739 = and i1 %59, %738
  br label %740

740:                                              ; preds = %737, %734
  %741 = phi i1 [ true, %734 ], [ %739, %737 ]
  %742 = or i1 %735, %741
  %743 = icmp sgt i32 %570, 0
  br i1 %742, label %744, label %819

744:                                              ; preds = %740
  br i1 %743, label %745, label %747

745:                                              ; preds = %744
  %746 = zext i32 %570 to i64
  br label %750

747:                                              ; preds = %750, %744
  %748 = phi i64 [ 0, %744 ], [ %759, %750 ]
  %749 = icmp eq i64 %748, 0
  br i1 %749, label %770, label %762

750:                                              ; preds = %750, %745
  %751 = phi i64 [ 0, %745 ], [ %760, %750 ]
  %752 = phi i64 [ 0, %745 ], [ %759, %750 ]
  %753 = getelementptr inbounds [1400 x i32], ptr %558, i64 0, i64 %751
  %754 = load i32, ptr %753, align 4, !tbaa !24
  %755 = zext i32 %754 to i64
  %756 = shl nuw nsw i64 %755, 1
  %757 = add nuw nsw i64 %756, %752
  %758 = trunc i64 %757 to i32
  store i32 %758, ptr %753, align 4, !tbaa !24
  %759 = lshr i64 %757, 32
  %760 = add nuw nsw i64 %751, 1
  %761 = icmp eq i64 %760, %746
  br i1 %761, label %747, label %750
762:                                              ; preds = %747
  %763 = icmp eq i32 %570, 1400
  br i1 %763, label %764, label %765

764:                                              ; preds = %762
  store i32 %565, ptr %88, align 4
  tail call void @llvm.trap()
  unreachable

765:                                              ; preds = %762
  %766 = trunc i64 %748 to i32
  %767 = add nsw i32 %570, 1
  store i32 %767, ptr %5, align 4, !tbaa !34
  %768 = sext i32 %570 to i64
  %769 = getelementptr inbounds [1400 x i32], ptr %558, i64 0, i64 %768
  store i32 %766, ptr %769, align 4, !tbaa !24
  br label %770

770:                                              ; preds = %765, %747
  %771 = load i32, ptr %5, align 4, !tbaa !34
  %772 = icmp eq i32 %771, %557
  br i1 %772, label %773, label %777

773:                                              ; preds = %770
  %774 = icmp eq i32 %771, 0
  br i1 %774, label %793, label %775

775:                                              ; preds = %773
  %776 = sext i32 %771 to i64
  br label %782

777:                                              ; preds = %770
  %778 = icmp slt i32 %771, %557
  %779 = select i1 %778, i32 -1, i32 1
  br label %793

780:                                              ; preds = %782
  %781 = icmp eq i64 %784, 0
  br i1 %781, label %793, label %782
782:                                              ; preds = %775, %780
  %783 = phi i64 [ %776, %775 ], [ %784, %780 ]
  %784 = add nsw i64 %783, -1
  %785 = getelementptr inbounds [1400 x i32], ptr %558, i64 0, i64 %784
  %786 = load i32, ptr %785, align 4, !tbaa !24
  %787 = getelementptr inbounds [1400 x i32], ptr %84, i64 0, i64 %784
  %788 = load i32, ptr %787, align 4, !tbaa !24
  %789 = icmp eq i32 %786, %788
  br i1 %789, label %780, label %790
790:                                              ; preds = %782
  %791 = icmp ult i32 %786, %788
  %792 = select i1 %791, i32 -1, i32 1
  br label %793

793:                                              ; preds = %780, %773, %790, %777
  %794 = phi i32 [ %779, %777 ], [ %792, %790 ], [ 0, %773 ], [ 0, %780 ]
  br i1 %741, label %795, label %805

795:                                              ; preds = %793
  %796 = icmp slt i32 %794, 1
  %797 = and i1 %735, %796
  br i1 %797, label %798, label %803

798:                                              ; preds = %795
  %799 = icmp ne i32 %794, 0
  %800 = and i128 %640, 1
  %801 = icmp eq i128 %800, 0
  %802 = select i1 %799, i1 true, i1 %801
  br i1 %802, label %805, label %803

803:                                              ; preds = %798, %795
  %804 = add i128 %641, 1
  br label %805

805:                                              ; preds = %803, %798, %793
  %806 = phi i128 [ %804, %803 ], [ %641, %798 ], [ %641, %793 ]
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %10) #14
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %10, i8 0, i64 5604, i1 false), !alias.scope !119
  %807 = icmp eq i128 %806, 0
  br i1 %807, label %817, label %808

808:                                              ; preds = %805, %808
  %809 = phi i128 [ %815, %808 ], [ %806, %805 ]
  %810 = trunc i128 %809 to i32
  %811 = load i32, ptr %10, align 4, !tbaa !34, !alias.scope !119
  %812 = add nsw i32 %811, 1
  store i32 %812, ptr %10, align 4, !tbaa !34, !alias.scope !119
  %813 = sext i32 %811 to i64
  %814 = getelementptr inbounds [1400 x i32], ptr %561, i64 0, i64 %813
  store i32 %810, ptr %814, align 4, !tbaa !24, !alias.scope !119
  %815 = lshr i128 %809, 32
  %816 = icmp ult i128 %809, 4294967296
  br i1 %816, label %817, label %808
817:                                              ; preds = %808, %805
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %11, ptr noundef nonnull align 4 dereferenceable(5604) %10, i64 5604, i1 false), !tbaa.struct !30
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %10) #14
  %818 = sub i32 %562, %566
  br label %899

819:                                              ; preds = %740
  br i1 %743, label %820, label %822

820:                                              ; preds = %819
  %821 = zext i32 %570 to i64
  br label %825

822:                                              ; preds = %825, %819
  %823 = phi i64 [ 0, %819 ], [ %834, %825 ]
  %824 = icmp eq i64 %823, 0
  br i1 %824, label %845, label %837

825:                                              ; preds = %825, %820
  %826 = phi i64 [ 0, %820 ], [ %835, %825 ]
  %827 = phi i64 [ 0, %820 ], [ %834, %825 ]
  %828 = getelementptr inbounds [1400 x i32], ptr %558, i64 0, i64 %826
  %829 = load i32, ptr %828, align 4, !tbaa !24
  %830 = zext i32 %829 to i64
  %831 = mul nuw nsw i64 %830, 10
  %832 = add nuw nsw i64 %831, %827
  %833 = trunc i64 %832 to i32
  store i32 %833, ptr %828, align 4, !tbaa !24
  %834 = lshr i64 %832, 32
  %835 = add nuw nsw i64 %826, 1
  %836 = icmp eq i64 %835, %821
  br i1 %836, label %822, label %825
837:                                              ; preds = %822
  %838 = icmp eq i32 %570, 1400
  br i1 %838, label %839, label %840

839:                                              ; preds = %837
  store i32 %565, ptr %88, align 4
  tail call void @llvm.trap()
  unreachable

840:                                              ; preds = %837
  %841 = trunc i64 %823 to i32
  %842 = add nsw i32 %570, 1
  store i32 %842, ptr %5, align 4, !tbaa !34
  %843 = sext i32 %570 to i64
  %844 = getelementptr inbounds [1400 x i32], ptr %558, i64 0, i64 %843
  store i32 %841, ptr %844, align 4, !tbaa !24
  br label %845

845:                                              ; preds = %840, %822
  %846 = icmp sgt i32 %642, 0
  br i1 %846, label %847, label %849

847:                                              ; preds = %845
  %848 = zext i32 %642 to i64
  br label %852

849:                                              ; preds = %852, %845
  %850 = phi i64 [ 0, %845 ], [ %861, %852 ]
  %851 = icmp eq i64 %850, 0
  br i1 %851, label %872, label %864

852:                                              ; preds = %852, %847
  %853 = phi i64 [ 0, %847 ], [ %862, %852 ]
  %854 = phi i64 [ 0, %847 ], [ %861, %852 ]
  %855 = getelementptr inbounds [1400 x i32], ptr %93, i64 0, i64 %853
  %856 = load i32, ptr %855, align 4, !tbaa !24
  %857 = zext i32 %856 to i64
  %858 = mul nuw nsw i64 %857, 10
  %859 = add nuw nsw i64 %858, %854
  %860 = trunc i64 %859 to i32
  store i32 %860, ptr %855, align 4, !tbaa !24
  %861 = lshr i64 %859, 32
  %862 = add nuw nsw i64 %853, 1
  %863 = icmp eq i64 %862, %848
  br i1 %863, label %849, label %852
864:                                              ; preds = %849
  %865 = icmp eq i32 %642, 1400
  br i1 %865, label %866, label %867

866:                                              ; preds = %864
  store i32 %565, ptr %88, align 4
  tail call void @llvm.trap()
  unreachable

867:                                              ; preds = %864
  %868 = trunc i64 %850 to i32
  %869 = add nsw i32 %642, 1
  store i32 %869, ptr %7, align 4, !tbaa !34
  %870 = sext i32 %642 to i64
  %871 = getelementptr inbounds [1400 x i32], ptr %93, i64 0, i64 %870
  store i32 %868, ptr %871, align 4, !tbaa !24
  br label %872

872:                                              ; preds = %867, %849
  %873 = icmp sgt i32 %706, 0
  br i1 %873, label %874, label %876

874:                                              ; preds = %872
  %875 = zext i32 %706 to i64
  br label %879

876:                                              ; preds = %879, %872
  %877 = phi i64 [ 0, %872 ], [ %888, %879 ]
  %878 = icmp eq i64 %877, 0
  br i1 %878, label %899, label %891

879:                                              ; preds = %879, %874
  %880 = phi i64 [ 0, %874 ], [ %889, %879 ]
  %881 = phi i64 [ 0, %874 ], [ %888, %879 ]
  %882 = getelementptr inbounds [1400 x i32], ptr %94, i64 0, i64 %880
  %883 = load i32, ptr %882, align 4, !tbaa !24
  %884 = zext i32 %883 to i64
  %885 = mul nuw nsw i64 %884, 10
  %886 = add nuw nsw i64 %885, %881
  %887 = trunc i64 %886 to i32
  store i32 %887, ptr %882, align 4, !tbaa !24
  %888 = lshr i64 %886, 32
  %889 = add nuw nsw i64 %880, 1
  %890 = icmp eq i64 %889, %875
  br i1 %890, label %876, label %879
891:                                              ; preds = %876
  %892 = icmp eq i32 %706, 1400
  br i1 %892, label %893, label %894

893:                                              ; preds = %891
  store i32 %565, ptr %88, align 4
  tail call void @llvm.trap()
  unreachable

894:                                              ; preds = %891
  %895 = trunc i64 %877 to i32
  %896 = add nsw i32 %706, 1
  store i32 %896, ptr %8, align 4, !tbaa !34
  %897 = sext i32 %706 to i64
  %898 = getelementptr inbounds [1400 x i32], ptr %94, i64 0, i64 %897
  store i32 %895, ptr %898, align 4, !tbaa !24
  br label %899

899:                                              ; preds = %894, %876, %817
  %900 = phi i32 [ %818, %817 ], [ %565, %876 ], [ %565, %894 ]
  %901 = phi i128 [ %806, %817 ], [ %641, %876 ], [ %641, %894 ]
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %9) #14
  %902 = add nuw nsw i32 %566, 1
  br i1 %742, label %903, label %564
903:                                              ; preds = %899
  store i32 %900, ptr %88, align 4
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %8) #14
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %7) #14
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %6) #14
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %5) #14
  br label %904

904:                                              ; preds = %903, %44
  call void @llvm.lifetime.start.p0(i64 48, ptr nonnull %12) #14
  %905 = getelementptr inbounds i8, ptr %11, i64 4
  br label %906

906:                                              ; preds = %944, %904
  %907 = phi i32 [ %951, %944 ], [ -1, %904 ]
  %908 = phi i32 [ %950, %944 ], [ -1, %904 ]
  %909 = phi i32 [ %949, %944 ], [ 0, %904 ]
  %910 = phi i64 [ %945, %944 ], [ 0, %904 ]
  %911 = load i32, ptr %11, align 4, !tbaa !34
  %912 = icmp eq i32 %911, 0
  br i1 %912, label %918, label %913

913:                                              ; preds = %906
  %914 = sext i32 %911 to i64
  br label %931

915:                                              ; preds = %931
  %916 = trunc i64 %942 to i8
  %917 = or i8 %916, 48
  br label %918

918:                                              ; preds = %915, %906
  %919 = phi i8 [ 48, %906 ], [ %917, %915 ]
  br i1 %912, label %944, label %920

920:                                              ; preds = %918
  %921 = sext i32 %911 to i64
  br label %922

922:                                              ; preds = %928, %920
  %923 = phi i64 [ %921, %920 ], [ %924, %928 ]
  %924 = add nsw i64 %923, -1
  %925 = getelementptr inbounds [1400 x i32], ptr %905, i64 0, i64 %924
  %926 = load i32, ptr %925, align 4, !tbaa !24
  %927 = icmp eq i32 %926, 0
  br i1 %927, label %928, label %944

928:                                              ; preds = %922
  %929 = trunc i64 %924 to i32
  store i32 %929, ptr %11, align 4, !tbaa !34
  %930 = icmp eq i64 %924, 0
  br i1 %930, label %944, label %922
931:                                              ; preds = %931, %913
  %932 = phi i64 [ %914, %913 ], [ %934, %931 ]
  %933 = phi i64 [ 0, %913 ], [ %942, %931 ]
  %934 = add nsw i64 %932, -1
  %935 = shl nuw nsw i64 %933, 32
  %936 = getelementptr inbounds [1400 x i32], ptr %905, i64 0, i64 %934
  %937 = load i32, ptr %936, align 4, !tbaa !24
  %938 = zext i32 %937 to i64
  %939 = or i64 %935, %938
  %940 = udiv i64 %939, 10
  %941 = trunc i64 %940 to i32
  store i32 %941, ptr %936, align 4, !tbaa !24
  %942 = urem i64 %939, 10
  %943 = icmp eq i64 %934, 0
  br i1 %943, label %915, label %931
944:                                              ; preds = %922, %928, %918
  %945 = add nuw nsw i64 %910, 1
  %946 = getelementptr inbounds [48 x i8], ptr %12, i64 0, i64 %910
  store i8 %919, ptr %946, align 1, !tbaa !26
  %947 = load i32, ptr %11, align 4, !tbaa !25
  %948 = icmp eq i32 %947, 0
  %949 = add nuw i32 %909, 1
  %950 = add i32 %908, 1
  %951 = add i32 %907, -1
  br i1 %948, label %952, label %906
952:                                              ; preds = %944
  %953 = trunc i64 %910 to i32
  %954 = trunc i64 %945 to i32
  %955 = getelementptr inbounds i8, ptr %2, i64 28
  %956 = load i32, ptr %955, align 4, !tbaa !18
  %957 = icmp eq i32 %956, 0
  %958 = icmp ne i64 %910, 0
  %959 = select i1 %957, i1 %958, i1 false
  br i1 %959, label %960, label %980

960:                                              ; preds = %952
  %961 = getelementptr inbounds i8, ptr %11, i64 5604
  %962 = load i32, ptr %961, align 4
  %963 = add i32 %962, %909
  %964 = zext i32 %909 to i64
  br label %965

965:                                              ; preds = %960, %971
  %966 = phi i64 [ 0, %960 ], [ %973, %971 ]
  %967 = phi i32 [ %962, %960 ], [ %972, %971 ]
  %968 = getelementptr inbounds [48 x i8], ptr %12, i64 0, i64 %966
  %969 = load i8, ptr %968, align 1, !tbaa !26
  %970 = icmp eq i8 %969, 48
  br i1 %970, label %971, label %975

971:                                              ; preds = %965
  %972 = add nsw i32 %967, 1
  %973 = add nuw nsw i64 %966, 1
  %974 = icmp eq i64 %973, %964
  br i1 %974, label %977, label %965
975:                                              ; preds = %965
  %976 = trunc i64 %966 to i32
  br label %977

977:                                              ; preds = %971, %975
  %978 = phi i32 [ %967, %975 ], [ %963, %971 ]
  %979 = phi i32 [ %976, %975 ], [ %950, %971 ]
  store i32 %978, ptr %961, align 4
  br label %980

980:                                              ; preds = %977, %952
  %981 = phi i32 [ 0, %952 ], [ %979, %977 ]
  %982 = sub nsw i32 %954, %981
  %983 = getelementptr inbounds i8, ptr %11, i64 5604
  %984 = load i32, ptr %983, align 4, !tbaa !35
  %985 = add nsw i32 %984, %982
  %986 = add nsw i32 %985, -1
  br i1 %957, label %987, label %991

987:                                              ; preds = %980
  %988 = icmp slt i32 %985, -5
  %989 = icmp sgt i32 %984, 6
  %990 = or i1 %989, %988
  br i1 %990, label %1051, label %991

991:                                              ; preds = %987, %980
  %992 = icmp slt i32 %985, 1
  br i1 %992, label %993, label %1013

993:                                              ; preds = %991
  %994 = zext i32 %22 to i64
  %995 = getelementptr inbounds i8, ptr %0, i64 %994
  store i8 48, ptr %995, align 1, !tbaa !26
  %996 = or i32 %22, 2
  %997 = getelementptr inbounds i8, ptr %995, i64 1
  store i8 46, ptr %997, align 1, !tbaa !26
  %998 = icmp slt i32 %985, 0
  br i1 %998, label %999, label %1013

999:                                              ; preds = %993
  %1000 = zext i32 %996 to i64
  %1001 = sub i32 %981, %984
  %1002 = add i32 %1001, %907
  %1003 = tail call i32 @llvm.smax.i32(i32 %1002, i32 1)
  %1004 = add nuw i32 %996, %1003
  %1005 = zext i32 %1004 to i64
  br label %1006

1006:                                             ; preds = %999, %1006
  %1007 = phi i64 [ %1000, %999 ], [ %1008, %1006 ]
  %1008 = add nuw nsw i64 %1007, 1
  %1009 = getelementptr inbounds i8, ptr %0, i64 %1007
  store i8 48, ptr %1009, align 1, !tbaa !26
  %1010 = icmp eq i64 %1008, %1005
  br i1 %1010, label %1011, label %1006
1011:                                             ; preds = %1006
  %1012 = trunc i64 %1008 to i32
  br label %1013

1013:                                             ; preds = %1011, %993, %991
  %1014 = phi i32 [ %22, %991 ], [ %996, %993 ], [ %1012, %1011 ]
  %1015 = icmp sgt i32 %981, %953
  br i1 %1015, label %1018, label %1016

1016:                                             ; preds = %1013
  %1017 = sext i32 %981 to i64
  br label %1024

1018:                                             ; preds = %1041, %1013
  %1019 = phi i32 [ %1014, %1013 ], [ %1042, %1041 ]
  %1020 = phi i32 [ %985, %1013 ], [ %984, %1041 ]
  %1021 = icmp sgt i32 %1020, 0
  br i1 %1021, label %1022, label %1119

1022:                                             ; preds = %1018
  %1023 = sext i32 %1019 to i64
  br label %1044

1024:                                             ; preds = %1016, %1041
  %1025 = phi i64 [ %910, %1016 ], [ %1043, %1041 ]
  %1026 = phi i32 [ %985, %1016 ], [ %1033, %1041 ]
  %1027 = phi i32 [ %1014, %1016 ], [ %1042, %1041 ]
  %1028 = getelementptr inbounds [48 x i8], ptr %12, i64 0, i64 %1025
  %1029 = load i8, ptr %1028, align 1, !tbaa !26
  %1030 = add nsw i32 %1027, 1
  %1031 = sext i32 %1027 to i64
  %1032 = getelementptr inbounds i8, ptr %0, i64 %1031
  store i8 %1029, ptr %1032, align 1, !tbaa !26
  %1033 = add nsw i32 %1026, -1
  %1034 = icmp eq i32 %1033, 0
  %1035 = icmp sgt i64 %1025, %1017
  %1036 = and i1 %1034, %1035
  br i1 %1036, label %1037, label %1041

1037:                                             ; preds = %1024
  %1038 = add nsw i32 %1027, 2
  %1039 = sext i32 %1030 to i64
  %1040 = getelementptr inbounds i8, ptr %0, i64 %1039
  store i8 46, ptr %1040, align 1, !tbaa !26
  br label %1041

1041:                                             ; preds = %1024, %1037
  %1042 = phi i32 [ %1038, %1037 ], [ %1030, %1024 ]
  %1043 = add nsw i64 %1025, -1
  br i1 %1035, label %1024, label %1018
1044:                                             ; preds = %1022, %1044
  %1045 = phi i64 [ %1023, %1022 ], [ %1048, %1044 ]
  %1046 = phi i32 [ %1020, %1022 ], [ %1047, %1044 ]
  %1047 = add nsw i32 %1046, -1
  %1048 = add nsw i64 %1045, 1
  %1049 = getelementptr inbounds i8, ptr %0, i64 %1045
  store i8 48, ptr %1049, align 1, !tbaa !26
  %1050 = icmp sgt i32 %1046, 1
  br i1 %1050, label %1044, label %1117
1051:                                             ; preds = %987
  %1052 = add nuw nsw i32 %22, 1
  %1053 = zext i32 %22 to i64
  %1054 = getelementptr inbounds i8, ptr %0, i64 %1053
  store i8 %919, ptr %1054, align 1, !tbaa !26
  %1055 = icmp sgt i32 %982, 1
  br i1 %1055, label %1056, label %1060

1056:                                             ; preds = %1051
  %1057 = or i32 %22, 2
  %1058 = zext i32 %1052 to i64
  %1059 = getelementptr inbounds i8, ptr %0, i64 %1058
  store i8 46, ptr %1059, align 1, !tbaa !26
  br label %1060

1060:                                             ; preds = %1056, %1051
  %1061 = phi i32 [ %1057, %1056 ], [ %1052, %1051 ]
  %1062 = icmp slt i32 %981, %953
  br i1 %1062, label %1063, label %1070

1063:                                             ; preds = %1060
  %1064 = zext i32 %1061 to i64
  %1065 = sub i32 %1061, %981
  %1066 = add i32 %1065, %909
  %1067 = zext i32 %1066 to i64
  br label %1108

1068:                                             ; preds = %1108
  %1069 = trunc i64 %1114 to i32
  br label %1070

1070:                                             ; preds = %1068, %1060
  %1071 = phi i32 [ %1061, %1060 ], [ %1069, %1068 ]
  %1072 = sext i32 %1071 to i64
  %1073 = getelementptr inbounds i8, ptr %0, i64 %1072
  store i8 101, ptr %1073, align 1, !tbaa !26
  %1074 = icmp slt i32 %985, 1
  %1075 = select i1 %1074, i8 45, i8 43
  %1076 = getelementptr i8, ptr %1073, i64 1
  store i8 %1075, ptr %1076, align 1, !tbaa !26
  %1077 = sub nsw i32 1, %985
  %1078 = select i1 %1074, i32 %1077, i32 %986
  call void @llvm.lifetime.start.p0(i64 12, ptr nonnull %4) #14
  br label %1079

1079:                                             ; preds = %1079, %1070
  %1080 = phi i32 [ %1090, %1079 ], [ 1, %1070 ]
  %1081 = phi i64 [ %1086, %1079 ], [ 0, %1070 ]
  %1082 = phi i32 [ %1088, %1079 ], [ %1078, %1070 ]
  %1083 = urem i32 %1082, 10
  %1084 = trunc i32 %1083 to i8
  %1085 = or i8 %1084, 48
  %1086 = add nuw nsw i64 %1081, 1
  %1087 = getelementptr inbounds [12 x i8], ptr %4, i64 0, i64 %1081
  store i8 %1085, ptr %1087, align 1, !tbaa !26
  %1088 = udiv i32 %1082, 10
  %1089 = icmp ult i32 %1082, 10
  %1090 = add nuw i32 %1080, 1
  br i1 %1089, label %1091, label %1079
1091:                                             ; preds = %1079
  %1092 = add nsw i32 %1071, 2
  %1093 = sext i32 %1092 to i64
  %1094 = getelementptr inbounds i8, ptr %0, i64 %1093
  %1095 = and i64 %1081, 4294967295
  %1096 = zext i32 %1080 to i64
  br label %1097

1097:                                             ; preds = %1097, %1091
  %1098 = phi i64 [ 0, %1091 ], [ %1103, %1097 ]
  %1099 = sub nuw nsw i64 %1095, %1098
  %1100 = getelementptr inbounds [12 x i8], ptr %4, i64 0, i64 %1099
  %1101 = load i8, ptr %1100, align 1, !tbaa !26
  %1102 = getelementptr inbounds i8, ptr %1094, i64 %1098
  store i8 %1101, ptr %1102, align 1, !tbaa !26
  %1103 = add nuw nsw i64 %1098, 1
  %1104 = icmp eq i64 %1103, %1096
  br i1 %1104, label %1105, label %1097
1105:                                             ; preds = %1097
  %1106 = trunc i64 %1086 to i32
  call void @llvm.lifetime.end.p0(i64 12, ptr nonnull %4) #14
  %1107 = add nsw i32 %1092, %1106
  br label %1119

1108:                                             ; preds = %1063, %1108
  %1109 = phi i64 [ %1064, %1063 ], [ %1114, %1108 ]
  %1110 = phi i64 [ %910, %1063 ], [ %1111, %1108 ]
  %1111 = add nsw i64 %1110, -1
  %1112 = getelementptr inbounds [48 x i8], ptr %12, i64 0, i64 %1111
  %1113 = load i8, ptr %1112, align 1, !tbaa !26
  %1114 = add nuw nsw i64 %1109, 1
  %1115 = getelementptr inbounds i8, ptr %0, i64 %1109
  store i8 %1113, ptr %1115, align 1, !tbaa !26
  %1116 = icmp eq i64 %1114, %1067
  br i1 %1116, label %1068, label %1108
1117:                                             ; preds = %1044
  %1118 = trunc i64 %1048 to i32
  br label %1119

1119:                                             ; preds = %1117, %1018, %1105
  %1120 = phi i32 [ %1107, %1105 ], [ %1019, %1018 ], [ %1118, %1117 ]
  call void @llvm.lifetime.end.p0(i64 48, ptr nonnull %12) #14
  br label %1123

1121:                                             ; preds = %28
  %1122 = trunc i64 %33 to i32
  br label %1123

1123:                                             ; preds = %1121, %1119, %40
  %1124 = phi i32 [ %1120, %1119 ], [ %41, %40 ], [ %1122, %1121 ]
  call void @llvm.lifetime.end.p0(i64 5616, ptr nonnull %11) #14
  ret i32 %1124
}

; Function Attrs: nounwind
define weak hidden i32 @tz_soft_parse(ptr noundef writeonly %0, ptr noundef readonly %1, i64 noundef %2, i32 noundef %3) local_unnamed_addr #6 {
  %5 = alloca %struct.tzrt_format, align 8
  %6 = alloca i32, align 4
  %7 = add i64 %2, -4097
  %8 = icmp ult i64 %7, -4096
  br i1 %8, label %359, label %9

9:                                                ; preds = %4
  call void @llvm.lifetime.start.p0(i64 36, ptr nonnull %5) #14
  switch i32 %3, label %73 [
    i32 0, label %10
    i32 1, label %19
    i32 2, label %28
    i32 3, label %37
    i32 4, label %46
    i32 5, label %55
    i32 6, label %64
  ]

10:                                               ; preds = %9
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !131
  %11 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 11, ptr %11, align 4, !tbaa !12, !alias.scope !131
  %12 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -24, ptr %12, align 8, !tbaa !13, !alias.scope !131
  %13 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 5, ptr %13, align 4, !tbaa !14, !alias.scope !131
  %14 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 10, ptr %14, align 8, !tbaa !15, !alias.scope !131
  %15 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 15, ptr %15, align 4, !tbaa !16, !alias.scope !131
  %16 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 16, ptr %16, align 8, !tbaa !17, !alias.scope !131
  %17 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %17, align 4, !tbaa !18, !alias.scope !131
  %18 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %18, align 8, !tbaa !19, !alias.scope !131
  br label %86

19:                                               ; preds = %9
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !131
  %20 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 24, ptr %20, align 4, !tbaa !12, !alias.scope !131
  %21 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -149, ptr %21, align 8, !tbaa !13, !alias.scope !131
  %22 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 104, ptr %22, align 4, !tbaa !14, !alias.scope !131
  %23 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 23, ptr %23, align 8, !tbaa !15, !alias.scope !131
  %24 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 127, ptr %24, align 4, !tbaa !16, !alias.scope !131
  %25 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 32, ptr %25, align 8, !tbaa !17, !alias.scope !131
  %26 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %26, align 4, !tbaa !18, !alias.scope !131
  %27 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %27, align 8, !tbaa !19, !alias.scope !131
  br label %86

28:                                               ; preds = %9
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !131
  %29 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 53, ptr %29, align 4, !tbaa !12, !alias.scope !131
  %30 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -1074, ptr %30, align 8, !tbaa !13, !alias.scope !131
  %31 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 971, ptr %31, align 4, !tbaa !14, !alias.scope !131
  %32 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 52, ptr %32, align 8, !tbaa !15, !alias.scope !131
  %33 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 1023, ptr %33, align 4, !tbaa !16, !alias.scope !131
  %34 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 64, ptr %34, align 8, !tbaa !17, !alias.scope !131
  %35 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %35, align 4, !tbaa !18, !alias.scope !131
  %36 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %36, align 8, !tbaa !19, !alias.scope !131
  br label %86

37:                                               ; preds = %9
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !131
  %38 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 113, ptr %38, align 4, !tbaa !12, !alias.scope !131
  %39 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -16494, ptr %39, align 8, !tbaa !13, !alias.scope !131
  %40 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 16271, ptr %40, align 4, !tbaa !14, !alias.scope !131
  %41 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 112, ptr %41, align 8, !tbaa !15, !alias.scope !131
  %42 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 16383, ptr %42, align 4, !tbaa !16, !alias.scope !131
  %43 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 128, ptr %43, align 8, !tbaa !17, !alias.scope !131
  %44 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %44, align 4, !tbaa !18, !alias.scope !131
  %45 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %45, align 8, !tbaa !19, !alias.scope !131
  br label %86

46:                                               ; preds = %9
  store i32 10, ptr %5, align 8, !tbaa !4, !alias.scope !131
  %47 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 7, ptr %47, align 4, !tbaa !12, !alias.scope !131
  %48 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -101, ptr %48, align 8, !tbaa !13, !alias.scope !131
  %49 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 90, ptr %49, align 4, !tbaa !14, !alias.scope !131
  %50 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 23, ptr %50, align 8, !tbaa !15, !alias.scope !131
  %51 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 101, ptr %51, align 4, !tbaa !16, !alias.scope !131
  %52 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 32, ptr %52, align 8, !tbaa !17, !alias.scope !131
  %53 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %53, align 4, !tbaa !18, !alias.scope !131
  %54 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %54, align 8, !tbaa !19, !alias.scope !131
  br label %86

55:                                               ; preds = %9
  store i32 10, ptr %5, align 8, !tbaa !4, !alias.scope !131
  %56 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 16, ptr %56, align 4, !tbaa !12, !alias.scope !131
  %57 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -398, ptr %57, align 8, !tbaa !13, !alias.scope !131
  %58 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 369, ptr %58, align 4, !tbaa !14, !alias.scope !131
  %59 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 53, ptr %59, align 8, !tbaa !15, !alias.scope !131
  %60 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 398, ptr %60, align 4, !tbaa !16, !alias.scope !131
  %61 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 64, ptr %61, align 8, !tbaa !17, !alias.scope !131
  %62 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %62, align 4, !tbaa !18, !alias.scope !131
  %63 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %63, align 8, !tbaa !19, !alias.scope !131
  br label %86

64:                                               ; preds = %9
  store i32 10, ptr %5, align 8, !tbaa !4, !alias.scope !131
  %65 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 34, ptr %65, align 4, !tbaa !12, !alias.scope !131
  %66 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 -6176, ptr %66, align 8, !tbaa !13, !alias.scope !131
  %67 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 6111, ptr %67, align 4, !tbaa !14, !alias.scope !131
  %68 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 113, ptr %68, align 8, !tbaa !15, !alias.scope !131
  %69 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 6176, ptr %69, align 4, !tbaa !16, !alias.scope !131
  %70 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 128, ptr %70, align 8, !tbaa !17, !alias.scope !131
  %71 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 0, ptr %71, align 4, !tbaa !18, !alias.scope !131
  %72 = getelementptr inbounds i8, ptr %5, i64 32
  store i32 1, ptr %72, align 8, !tbaa !19, !alias.scope !131
  br label %86

73:                                               ; preds = %9
  %74 = and i32 %3, 7
  %75 = shl nuw nsw i32 8, %74
  store i32 2, ptr %5, align 8, !tbaa !4, !alias.scope !131
  %76 = getelementptr inbounds i8, ptr %5, i64 4
  store i32 %75, ptr %76, align 4, !tbaa !12, !alias.scope !131
  %77 = getelementptr inbounds i8, ptr %5, i64 8
  store i32 0, ptr %77, align 8, !tbaa !13, !alias.scope !131
  %78 = getelementptr inbounds i8, ptr %5, i64 12
  store i32 0, ptr %78, align 4, !tbaa !14, !alias.scope !131
  %79 = getelementptr inbounds i8, ptr %5, i64 16
  store i32 0, ptr %79, align 8, !tbaa !15, !alias.scope !131
  %80 = getelementptr inbounds i8, ptr %5, i64 20
  store i32 0, ptr %80, align 4, !tbaa !16, !alias.scope !131
  %81 = getelementptr inbounds i8, ptr %5, i64 24
  store i32 %75, ptr %81, align 8, !tbaa !17, !alias.scope !131
  %82 = getelementptr inbounds i8, ptr %5, i64 28
  store i32 1, ptr %82, align 4, !tbaa !18, !alias.scope !131
  %83 = getelementptr inbounds i8, ptr %5, i64 32
  %84 = icmp slt i32 %3, 24
  %85 = zext i1 %84 to i32
  store i32 %85, ptr %83, align 8, !tbaa !19, !alias.scope !131
  br label %86

86:                                               ; preds = %10, %19, %28, %37, %46, %55, %64, %73
  %87 = phi i32 [ 1, %10 ], [ 1, %19 ], [ 1, %28 ], [ 1, %37 ], [ 1, %46 ], [ 1, %55 ], [ 1, %64 ], [ %85, %73 ]
  %88 = phi i1 [ true, %10 ], [ true, %19 ], [ true, %28 ], [ true, %37 ], [ true, %46 ], [ true, %55 ], [ true, %64 ], [ false, %73 ]
  %89 = phi i32 [ 16, %10 ], [ 32, %19 ], [ 64, %28 ], [ 128, %37 ], [ 32, %46 ], [ 64, %55 ], [ 128, %64 ], [ %75, %73 ]
  %90 = phi i32 [ 31, %10 ], [ 255, %19 ], [ 2047, %28 ], [ 32767, %37 ], [ 203, %46 ], [ 797, %55 ], [ 12353, %64 ], [ 1, %73 ]
  %91 = phi i32 [ 10, %10 ], [ 23, %19 ], [ 52, %28 ], [ 112, %37 ], [ 23, %46 ], [ 53, %55 ], [ 113, %64 ], [ 0, %73 ]
  %92 = phi i1 [ true, %10 ], [ true, %19 ], [ true, %28 ], [ true, %37 ], [ false, %46 ], [ false, %55 ], [ false, %64 ], [ true, %73 ]
  %93 = load i8, ptr %1, align 1, !tbaa !26
  %94 = icmp eq i8 %93, 45
  %95 = zext i1 %94 to i32
  %96 = icmp eq i8 %93, 43
  %97 = or i1 %94, %96
  %98 = zext i1 %97 to i64
  %99 = icmp eq i64 %2, %98
  br i1 %99, label %357, label %100

100:                                              ; preds = %86
  br i1 %88, label %294, label %101

101:                                              ; preds = %100
  %102 = icmp eq i32 %87, 0
  %103 = and i1 %102, %94
  br i1 %103, label %357, label %104

104:                                              ; preds = %101
  %105 = select i1 %97, i64 2, i64 1
  %106 = icmp ult i64 %105, %2
  br i1 %106, label %107, label %118

107:                                              ; preds = %104
  %108 = getelementptr inbounds i8, ptr %1, i64 %98
  %109 = load i8, ptr %108, align 1, !tbaa !26
  %110 = icmp eq i8 %109, 48
  br i1 %110, label %111, label %118

111:                                              ; preds = %107
  %112 = getelementptr inbounds i8, ptr %1, i64 %105
  %113 = load i8, ptr %112, align 1, !tbaa !26
  switch i8 %113, label %118 [
    i8 120, label %114
    i8 98, label %116
  ]

114:                                              ; preds = %111
  %115 = or i64 %98, 2
  br label %118

116:                                              ; preds = %111
  %117 = or i64 %98, 2
  br label %118

118:                                              ; preds = %111, %114, %116, %107, %104
  %119 = phi i1 [ false, %114 ], [ false, %116 ], [ true, %107 ], [ true, %104 ], [ true, %111 ]
  %120 = phi i32 [ 16, %114 ], [ 2, %116 ], [ 10, %107 ], [ 10, %104 ], [ 10, %111 ]
  %121 = phi i64 [ %115, %114 ], [ %117, %116 ], [ %98, %107 ], [ %98, %104 ], [ %98, %111 ]
  %122 = icmp eq i64 %121, %2
  br i1 %122, label %357, label %123

123:                                              ; preds = %118
  %124 = sub nuw nsw i32 %89, %87
  %125 = icmp eq i32 %124, 128
  %126 = zext i32 %124 to i128
  %127 = shl nsw i128 -1, %126
  %128 = xor i128 %127, -1
  %129 = select i1 %125, i128 -1, i128 %128
  %130 = icmp ne i32 %87, 0
  %131 = and i1 %130, %94
  %132 = zext i1 %131 to i128
  %133 = add i128 %129, %132
  %134 = icmp samesign ult i32 %89, 65
  br i1 %134, label %135, label %140

135:                                              ; preds = %123
  %136 = trunc i128 %133 to i64
  %137 = zext i32 %120 to i64
  %138 = udiv i64 %136, %137
  %139 = zext i64 %138 to i128
  br label %143

140:                                              ; preds = %123
  %141 = zext i32 %120 to i128
  %142 = udiv i128 %133, %141
  br label %143

143:                                              ; preds = %140, %135
  %144 = phi i128 [ %139, %135 ], [ %142, %140 ]
  br i1 %134, label %145, label %150

145:                                              ; preds = %143
  %146 = trunc i128 %133 to i64
  %147 = zext i32 %120 to i64
  %148 = urem i64 %146, %147
  %149 = trunc i64 %148 to i32
  br label %154

150:                                              ; preds = %143
  %151 = zext i32 %120 to i128
  %152 = urem i128 %133, %151
  %153 = trunc i128 %152 to i32
  br label %154

154:                                              ; preds = %150, %145
  %155 = phi i32 [ %149, %145 ], [ %153, %150 ]
  call void @llvm.lifetime.start.p0(i64 4, ptr nonnull %6) #14
  %156 = sub nsw i64 %2, %121
  %157 = icmp ugt i64 %156, 7
  %158 = select i1 %119, i1 %157, i1 false
  br i1 %158, label %159, label %228

159:                                              ; preds = %154
  %160 = getelementptr inbounds i8, ptr %1, i64 %121
  %161 = call fastcc i32 @eight_decimal_digits(ptr noundef nonnull %160, ptr noundef %6) #15
  %162 = icmp eq i32 %161, 0
  br i1 %162, label %228, label %163

163:                                              ; preds = %159
  br i1 %134, label %164, label %168

164:                                              ; preds = %163
  %165 = trunc i128 %133 to i64
  %166 = udiv i64 %165, 100000000
  %167 = zext i64 %166 to i128
  br label %170

168:                                              ; preds = %163
  %169 = udiv i128 %133, 100000000
  br label %170

170:                                              ; preds = %168, %164
  %171 = phi i128 [ %167, %164 ], [ %169, %168 ]
  br i1 %134, label %172, label %176

172:                                              ; preds = %170
  %173 = trunc i128 %133 to i64
  %174 = urem i64 %173, 100000000
  %175 = trunc i64 %174 to i32
  br label %179

176:                                              ; preds = %170
  %177 = urem i128 %133, 100000000
  %178 = trunc i128 %177 to i32
  br label %179

179:                                              ; preds = %176, %172
  %180 = phi i32 [ %175, %172 ], [ %178, %176 ]
  %181 = load i32, ptr %6, align 4
  %182 = icmp eq i128 %171, 0
  %183 = icmp ugt i32 %181, %180
  %184 = select i1 %182, i1 %183, i1 false
  br i1 %184, label %222, label %189

185:                                              ; preds = %207
  %186 = icmp eq i128 %195, %171
  %187 = icmp ult i32 %180, %219
  %188 = select i1 %186, i1 %187, i1 false
  br i1 %188, label %222, label %189
189:                                              ; preds = %179, %185
  %190 = phi i32 [ %219, %185 ], [ %181, %179 ]
  %191 = phi i128 [ %195, %185 ], [ 0, %179 ]
  %192 = phi i64 [ %196, %185 ], [ %121, %179 ]
  %193 = mul nuw i128 %191, 100000000
  %194 = zext i32 %190 to i128
  %195 = add i128 %193, %194
  %196 = add i64 %192, 8
  %197 = sub i64 %2, %196
  %198 = icmp ugt i64 %197, 7
  br i1 %198, label %199, label %222

199:                                              ; preds = %189
  %200 = getelementptr inbounds i8, ptr %1, i64 %196
  %201 = load i64, ptr %200, align 1
  %202 = add i64 %201, -3472328296227680304
  %203 = add i64 %201, 5063812098665367110
  %204 = or i64 %202, %203
  %205 = and i64 %204, -9187201950435737472
  %206 = icmp eq i64 %205, 0
  br i1 %206, label %207, label %222

207:                                              ; preds = %199
  %208 = mul i64 %202, 10
  %209 = lshr i64 %202, 8
  %210 = add i64 %208, %209
  %211 = and i64 %210, 71777214294589695
  %212 = mul nuw nsw i64 %211, 100
  %213 = lshr i64 %211, 16
  %214 = add nuw nsw i64 %212, %213
  %215 = and i64 %214, 281470681808895
  %216 = mul nuw nsw i64 %215, 10000
  %217 = lshr i64 %215, 32
  %218 = add nuw nsw i64 %216, %217
  %219 = trunc i64 %218 to i32
  %220 = icmp ugt i128 %195, %171
  br i1 %220, label %221, label %185
221:                                              ; preds = %207
  br label %222
222:                                              ; preds = %185, %189, %199, %221, %179
  %223 = phi i32 [ %219, %221 ], [ %181, %179 ], [ %219, %185 ], [ %190, %189 ], [ %190, %199 ]
  %224 = phi i32 [ 1, %221 ], [ 0, %179 ], [ 1, %199 ], [ 1, %189 ], [ 1, %185 ]
  %225 = phi i128 [ %195, %221 ], [ 0, %179 ], [ %195, %199 ], [ %195, %189 ], [ %195, %185 ]
  %226 = phi i1 [ false, %221 ], [ false, %179 ], [ false, %185 ], [ true, %189 ], [ true, %199 ]
  %227 = phi i64 [ %196, %221 ], [ %121, %179 ], [ %196, %199 ], [ %196, %189 ], [ %196, %185 ]
  store i32 %223, ptr %6, align 4
  br i1 %226, label %228, label %292

228:                                              ; preds = %222, %159, %154
  %229 = phi i32 [ %224, %222 ], [ 0, %159 ], [ 0, %154 ]
  %230 = phi i128 [ %225, %222 ], [ 0, %159 ], [ 0, %154 ]
  %231 = phi i64 [ %227, %222 ], [ %121, %159 ], [ %121, %154 ]
  %232 = icmp ult i64 %231, %2
  br i1 %232, label %233, label %285

233:                                              ; preds = %228
  %234 = zext i32 %120 to i128
  br label %235

235:                                              ; preds = %233, %280
  %236 = phi i64 [ %231, %233 ], [ %283, %280 ]
  %237 = phi i128 [ %230, %233 ], [ %282, %280 ]
  %238 = phi i32 [ %229, %233 ], [ %281, %280 ]
  %239 = getelementptr inbounds i8, ptr %1, i64 %236
  %240 = load i8, ptr %239, align 1, !tbaa !26
  %241 = icmp eq i8 %240, 95
  br i1 %241, label %242, label %247

242:                                              ; preds = %235
  %243 = icmp eq i32 %238, 0
  %244 = add nuw nsw i64 %236, 1
  %245 = icmp eq i64 %244, %2
  %246 = select i1 %243, i1 true, i1 %245
  br i1 %246, label %292, label %280

247:                                              ; preds = %235
  %248 = sext i8 %240 to i32
  %249 = add i8 %240, -48
  %250 = icmp ult i8 %249, 10
  br i1 %250, label %251, label %253

251:                                              ; preds = %247
  %252 = add nsw i32 %248, -48
  br label %263

253:                                              ; preds = %247
  %254 = add i8 %240, -97
  %255 = icmp ult i8 %254, 6
  br i1 %255, label %256, label %258

256:                                              ; preds = %253
  %257 = add nsw i32 %248, -87
  br label %263

258:                                              ; preds = %253
  %259 = add i8 %240, -65
  %260 = icmp ult i8 %259, 6
  %261 = add nsw i32 %248, -55
  %262 = select i1 %260, i32 %261, i32 -1
  br label %263

263:                                              ; preds = %251, %256, %258
  %264 = phi i32 [ %252, %251 ], [ %257, %256 ], [ %262, %258 ]
  %265 = icmp uge i32 %264, %120
  %266 = icmp ugt i128 %237, %144
  %267 = select i1 %265, i1 true, i1 %266
  br i1 %267, label %276, label %268

268:                                              ; preds = %263
  %269 = icmp eq i128 %237, %144
  %270 = icmp ugt i32 %264, %155
  %271 = select i1 %269, i1 %270, i1 false
  br i1 %271, label %276, label %272

272:                                              ; preds = %268
  %273 = mul i128 %237, %234
  %274 = zext i32 %264 to i128
  %275 = add i128 %273, %274
  br label %276

276:                                              ; preds = %263, %268, %272
  %277 = phi i32 [ 1, %272 ], [ %238, %268 ], [ %238, %263 ]
  %278 = phi i128 [ %275, %272 ], [ %237, %268 ], [ %237, %263 ]
  %279 = phi i1 [ true, %272 ], [ false, %268 ], [ false, %263 ]
  br i1 %279, label %280, label %292

280:                                              ; preds = %242, %276
  %281 = phi i32 [ %277, %276 ], [ 0, %242 ]
  %282 = phi i128 [ %278, %276 ], [ %237, %242 ]
  %283 = add i64 %236, 1
  %284 = icmp eq i64 %283, %2
  br i1 %284, label %285, label %235
285:                                              ; preds = %280, %228
  %286 = phi i128 [ %230, %228 ], [ %282, %280 ]
  %287 = sub i128 0, %286
  %288 = select i1 %94, i128 %287, i128 %286
  %289 = trunc i128 %288 to i64
  %290 = lshr i128 %288, 64
  %291 = trunc i128 %290 to i64
  tail call fastcc void @store(ptr noundef %0, i64 noundef %289, i64 noundef %291, i32 noundef %89) #15
  br label %292

292:                                              ; preds = %242, %276, %222, %285
  %293 = phi i32 [ 1, %285 ], [ 0, %222 ], [ 0, %276 ], [ 0, %242 ]
  call void @llvm.lifetime.end.p0(i64 4, ptr nonnull %6) #14
  br label %357

294:                                              ; preds = %100
  %295 = sub nuw nsw i64 %2, %98
  %296 = icmp eq i64 %295, 3
  br i1 %296, label %297, label %355

297:                                              ; preds = %294
  %298 = getelementptr inbounds i8, ptr %1, i64 %98
  %299 = load i8, ptr %298, align 1, !tbaa !26
  switch i8 %299, label %355 [
    i8 105, label %300
    i8 110, label %325
  ]

300:                                              ; preds = %297
  %301 = getelementptr inbounds i8, ptr %298, i64 1
  %302 = load i8, ptr %301, align 1, !tbaa !26
  %303 = icmp eq i8 %302, 110
  br i1 %303, label %304, label %323

304:                                              ; preds = %300
  %305 = getelementptr inbounds i8, ptr %298, i64 2
  %306 = load i8, ptr %305, align 1, !tbaa !26
  %307 = icmp eq i8 %306, 102
  br i1 %307, label %308, label %323

308:                                              ; preds = %304
  %309 = zext i1 %94 to i128
  %310 = add nsw i32 %89, -1
  %311 = zext i32 %310 to i128
  %312 = shl nuw i128 %309, %311
  %313 = zext i32 %90 to i128
  %314 = add nsw i32 %89, -6
  %315 = select i1 %92, i32 %91, i32 %314
  %316 = select i1 %92, i128 %313, i128 30
  %317 = zext i32 %315 to i128
  %318 = shl i128 %316, %317
  %319 = or i128 %312, %318
  %320 = trunc i128 %319 to i64
  %321 = lshr i128 %319, 64
  %322 = trunc i128 %321 to i64
  tail call fastcc void @store(ptr noundef %0, i64 noundef %320, i64 noundef %322, i32 noundef %89) #15
  br label %357

323:                                              ; preds = %304, %300
  %324 = icmp eq i8 %299, 110
  br i1 %324, label %325, label %355

325:                                              ; preds = %297, %323
  %326 = getelementptr inbounds i8, ptr %298, i64 1
  %327 = load i8, ptr %326, align 1, !tbaa !26
  %328 = icmp eq i8 %327, 97
  br i1 %328, label %329, label %355

329:                                              ; preds = %325
  %330 = getelementptr inbounds i8, ptr %298, i64 2
  %331 = load i8, ptr %330, align 1, !tbaa !26
  %332 = icmp eq i8 %331, 110
  br i1 %332, label %333, label %355

333:                                              ; preds = %329
  br i1 %92, label %334, label %345

334:                                              ; preds = %333
  %335 = zext i32 %90 to i128
  %336 = zext i32 %91 to i128
  %337 = shl nuw i128 %335, %336
  %338 = add nsw i32 %91, -1
  %339 = zext i32 %338 to i128
  %340 = shl nuw i128 1, %339
  %341 = or i128 %340, %337
  %342 = trunc i128 %341 to i64
  %343 = lshr i128 %341, 64
  %344 = trunc i128 %343 to i64
  br label %352

345:                                              ; preds = %333
  %346 = add nsw i32 %89, -6
  %347 = zext i32 %346 to i128
  %348 = shl i128 31, %347
  %349 = trunc i128 %348 to i64
  %350 = lshr i128 %348, 64
  %351 = trunc i128 %350 to i64
  br label %352

352:                                              ; preds = %334, %345
  %353 = phi i64 [ %342, %334 ], [ %349, %345 ]
  %354 = phi i64 [ %344, %334 ], [ %351, %345 ]
  tail call fastcc void @store(ptr noundef %0, i64 noundef %353, i64 noundef %354, i32 noundef %89) #15
  br label %357

355:                                              ; preds = %297, %329, %325, %323, %294
  %356 = tail call fastcc i32 @parse_float(ptr noundef %0, ptr noundef nonnull %1, i64 noundef %2, i64 noundef %98, i32 noundef %95, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %5) #15
  br label %357

357:                                              ; preds = %308, %352, %292, %118, %101, %86, %355
  %358 = phi i32 [ %356, %355 ], [ 0, %86 ], [ 0, %101 ], [ %293, %292 ], [ 0, %118 ], [ 1, %352 ], [ 1, %308 ]
  call void @llvm.lifetime.end.p0(i64 36, ptr nonnull %5) #14
  br label %359

359:                                              ; preds = %4, %357
  %360 = phi i32 [ %358, %357 ], [ 0, %4 ]
  ret i32 %360
}

; Function Attrs: mustprogress nofree norecurse nosync nounwind willreturn memory(argmem: readwrite)
define internal fastcc i32 @eight_decimal_digits(ptr noundef readonly %0, ptr noundef nonnull writeonly %1) unnamed_addr #7 {
  %3 = load i64, ptr %0, align 1
  %4 = add i64 %3, -3472328296227680304
  %5 = add i64 %3, 5063812098665367110
  %6 = or i64 %4, %5
  %7 = and i64 %6, -9187201950435737472
  %8 = icmp eq i64 %7, 0
  br i1 %8, label %9, label %22

9:                                                ; preds = %2
  %10 = mul i64 %4, 10
  %11 = lshr i64 %4, 8
  %12 = add i64 %10, %11
  %13 = and i64 %12, 71777214294589695
  %14 = mul nuw nsw i64 %13, 100
  %15 = lshr i64 %13, 16
  %16 = add nuw nsw i64 %14, %15
  %17 = and i64 %16, 281470681808895
  %18 = mul nuw nsw i64 %17, 10000
  %19 = lshr i64 %17, 32
  %20 = add nuw nsw i64 %18, %19
  %21 = trunc i64 %20 to i32
  store i32 %21, ptr %1, align 4, !tbaa !24
  br label %22

22:                                               ; preds = %2, %9
  %23 = phi i32 [ 1, %9 ], [ 0, %2 ]
  ret i32 %23
}

; Function Attrs: noinline nounwind
define internal fastcc i32 @parse_float(ptr noundef writeonly %0, ptr noundef readonly %1, i64 noundef %2, i64 noundef %3, i32 noundef %4, ptr noundef readonly byval(%struct.tzrt_format) align 8 %5) unnamed_addr #8 {
  %7 = alloca i64, align 8
  %8 = alloca %struct.tzrt_big, align 4
  %9 = alloca %struct.tzrt_big, align 4
  store i64 %3, ptr %7, align 8, !tbaa !136
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %8) #14
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %8, i8 0, i64 5604, i1 false), !alias.scope !138
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %9) #14
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %9, i8 0, i64 5604, i1 false), !alias.scope !141
  %10 = getelementptr inbounds i8, ptr %9, i64 4
  store i32 1, ptr %9, align 4, !tbaa !34, !alias.scope !141
  store i32 1, ptr %10, align 4, !tbaa !24, !alias.scope !141
  %11 = call fastcc i32 @decimal_digits(ptr noundef %1, i64 noundef %2, ptr noundef %7, ptr noundef nonnull %8, ptr noundef null) #15
  %12 = icmp slt i32 %11, 0
  br i1 %12, label %241, label %13

13:                                               ; preds = %6
  %14 = load i64, ptr %7, align 8, !tbaa !136
  %15 = icmp ult i64 %14, %2
  br i1 %15, label %16, label %24

16:                                               ; preds = %13
  %17 = getelementptr inbounds i8, ptr %1, i64 %14
  %18 = load i8, ptr %17, align 1, !tbaa !26
  %19 = icmp eq i8 %18, 46
  br i1 %19, label %20, label %24

20:                                               ; preds = %16
  %21 = add nuw nsw i64 %14, 1
  store i64 %21, ptr %7, align 8, !tbaa !136
  %22 = call fastcc i32 @decimal_digits(ptr noundef %1, i64 noundef %2, ptr noundef %7, ptr noundef nonnull %8, ptr noundef null) #15
  %23 = icmp slt i32 %22, 0
  br i1 %23, label %241, label %24

24:                                               ; preds = %20, %16, %13
  %25 = phi i32 [ %22, %20 ], [ 0, %16 ], [ 0, %13 ]
  %26 = or i32 %25, %11
  %27 = icmp eq i32 %26, 0
  br i1 %27, label %241, label %28

28:                                               ; preds = %24
  %29 = load i64, ptr %7, align 8, !tbaa !136
  %30 = icmp ult i64 %29, %2
  br i1 %30, label %31, label %104

31:                                               ; preds = %28
  %32 = getelementptr inbounds i8, ptr %1, i64 %29
  %33 = load i8, ptr %32, align 1, !tbaa !26
  switch i8 %33, label %104 [
    i8 101, label %34
    i8 69, label %34
  ]

34:                                               ; preds = %31, %31
  %35 = add i64 %29, 1
  store i64 %35, ptr %7, align 8, !tbaa !136
  %36 = icmp ult i64 %35, %2
  br i1 %36, label %37, label %41

37:                                               ; preds = %34
  %38 = getelementptr inbounds i8, ptr %1, i64 %35
  %39 = load i8, ptr %38, align 1, !tbaa !26
  %40 = icmp eq i8 %39, 45
  br label %41

41:                                               ; preds = %37, %34
  %42 = phi i1 [ false, %34 ], [ %40, %37 ]
  br i1 %36, label %43, label %50

43:                                               ; preds = %41
  br i1 %42, label %48, label %44

44:                                               ; preds = %43
  %45 = getelementptr inbounds i8, ptr %1, i64 %35
  %46 = load i8, ptr %45, align 1, !tbaa !26
  %47 = icmp eq i8 %46, 43
  br i1 %47, label %48, label %50

48:                                               ; preds = %44, %43
  %49 = add i64 %29, 2
  store i64 %49, ptr %7, align 8, !tbaa !136
  br label %50

50:                                               ; preds = %48, %44, %41
  %51 = load i64, ptr %7, align 8, !tbaa !136
  %52 = icmp ult i64 %51, %2
  br i1 %52, label %53, label %96

53:                                               ; preds = %50, %90
  %54 = phi i64 [ %85, %90 ], [ %51, %50 ]
  %55 = phi i32 [ %86, %90 ], [ 0, %50 ]
  %56 = phi i32 [ %88, %90 ], [ 0, %50 ]
  %57 = phi i64 [ %87, %90 ], [ %51, %50 ]
  %58 = getelementptr inbounds i8, ptr %1, i64 %57
  %59 = load i8, ptr %58, align 1, !tbaa !26
  %60 = icmp eq i8 %59, 95
  br i1 %60, label %61, label %72

61:                                               ; preds = %53
  %62 = icmp eq i32 %56, 0
  br i1 %62, label %84, label %63

63:                                               ; preds = %61
  %64 = add nuw nsw i64 %57, 1
  %65 = icmp eq i64 %64, %2
  br i1 %65, label %84, label %66

66:                                               ; preds = %63
  %67 = getelementptr inbounds i8, ptr %1, i64 %64
  %68 = load i8, ptr %67, align 1, !tbaa !26
  %69 = add i8 %68, -58
  %70 = icmp ult i8 %69, -10
  br i1 %70, label %84, label %71

71:                                               ; preds = %66
  br label %84
72:                                               ; preds = %53
  %73 = add i8 %59, -58
  %74 = icmp ult i8 %73, -10
  br i1 %74, label %84, label %75

75:                                               ; preds = %72
  %76 = add nsw i8 %59, -48
  %77 = zext i8 %76 to i32
  %78 = icmp slt i32 %55, 100000
  %79 = mul nsw i32 %55, 10
  %80 = add nsw i32 %79, %77
  %81 = select i1 %78, i32 %80, i32 %55
  %82 = add nsw i32 %56, 1
  %83 = add i64 %57, 1
  br label %84

84:                                               ; preds = %75, %72, %71, %66, %63, %61
  %85 = phi i64 [ %54, %61 ], [ %54, %63 ], [ %54, %66 ], [ %64, %71 ], [ %54, %72 ], [ %83, %75 ]
  %86 = phi i32 [ %55, %61 ], [ %55, %63 ], [ %55, %66 ], [ %55, %71 ], [ %55, %72 ], [ %81, %75 ]
  %87 = phi i64 [ %57, %61 ], [ %57, %63 ], [ %57, %66 ], [ %64, %71 ], [ %57, %72 ], [ %83, %75 ]
  %88 = phi i32 [ %56, %61 ], [ %56, %63 ], [ %56, %66 ], [ %56, %71 ], [ %56, %72 ], [ %82, %75 ]
  %89 = phi i32 [ 1, %61 ], [ 1, %63 ], [ 1, %66 ], [ 2, %71 ], [ 3, %72 ], [ 0, %75 ]
  switch i32 %89, label %92 [
    i32 0, label %90
    i32 2, label %90
    i32 3, label %93
    i32 1, label %95
  ]

90:                                               ; preds = %84, %84
  %91 = icmp ult i64 %87, %2
  br i1 %91, label %53, label %93
92:                                               ; preds = %84
  unreachable

93:                                               ; preds = %90, %84
  store i64 %85, ptr %7, align 8
  %94 = icmp sgt i32 %88, 0
  br label %96

95:                                               ; preds = %84
  store i64 %85, ptr %7, align 8
  br label %96

96:                                               ; preds = %95, %50, %93
  %97 = phi i32 [ %86, %93 ], [ 0, %50 ], [ %86, %95 ]
  %98 = phi i1 [ %94, %93 ], [ false, %50 ], [ false, %95 ]
  br i1 %98, label %99, label %241

99:                                               ; preds = %96
  %100 = sub nsw i32 0, %97
  %101 = select i1 %42, i32 %100, i32 %97
  %102 = load i64, ptr %7, align 8
  %103 = icmp eq i64 %102, %2
  br i1 %103, label %106, label %241

104:                                              ; preds = %31, %28
  %105 = icmp eq i64 %29, %2
  br i1 %105, label %106, label %241

106:                                              ; preds = %99, %104
  %107 = phi i32 [ 0, %104 ], [ %101, %99 ]
  %108 = sub nsw i32 %107, %25
  %109 = load i32, ptr %8, align 4, !tbaa !34
  %110 = icmp eq i32 %109, 0
  br i1 %110, label %111, label %142

111:                                              ; preds = %106
  %112 = call fastcc { i64, i64 } @pack(ptr noundef %8, ptr noundef %9, i32 noundef %108, i32 noundef %4, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %5) #15
  %113 = extractvalue { i64, i64 } %112, 0
  %114 = extractvalue { i64, i64 } %112, 1
  %115 = getelementptr inbounds i8, ptr %5, i64 24
  %116 = load i32, ptr %115, align 8, !tbaa !17
  %117 = zext i64 %114 to i128
  %118 = shl nuw i128 %117, 64
  %119 = zext i64 %113 to i128
  %120 = or i128 %118, %119
  switch i32 %116, label %121 [
    i32 8, label %126
    i32 16, label %128
    i32 32, label %130
    i32 64, label %132
    i32 128, label %133
  ]

121:                                              ; preds = %111
  %122 = icmp sgt i32 %116, 7
  br i1 %122, label %123, label %241

123:                                              ; preds = %121
  %124 = lshr i32 %116, 3
  %125 = zext i32 %124 to i64
  br label %134

126:                                              ; preds = %111
  %127 = trunc i64 %113 to i8
  store i8 %127, ptr %0, align 1, !tbaa !26
  br label %241

128:                                              ; preds = %111
  %129 = trunc i64 %113 to i16
  store i16 %129, ptr %0, align 1
  br label %241

130:                                              ; preds = %111
  %131 = trunc i64 %113 to i32
  store i32 %131, ptr %0, align 1
  br label %241

132:                                              ; preds = %111
  store i64 %113, ptr %0, align 1
  br label %241

133:                                              ; preds = %111
  store i128 %120, ptr %0, align 1
  br label %241

134:                                              ; preds = %134, %123
  %135 = phi i64 [ 0, %123 ], [ %140, %134 ]
  %136 = phi i128 [ %120, %123 ], [ %139, %134 ]
  %137 = trunc i128 %136 to i8
  %138 = getelementptr inbounds i8, ptr %0, i64 %135
  store i8 %137, ptr %138, align 1, !tbaa !26
  %139 = lshr i128 %136, 8
  %140 = add nuw nsw i64 %135, 1
  %141 = icmp eq i64 %140, %125
  br i1 %141, label %241, label %134
142:                                              ; preds = %106
  %143 = call fastcc i32 @magnitude(ptr noundef %8, ptr noundef %9, i32 noundef 10) #15
  %144 = add nsw i32 %143, %108
  %145 = load i32, ptr %5, align 8, !tbaa !4
  %146 = icmp eq i32 %145, 10
  %147 = getelementptr inbounds i8, ptr %5, i64 12
  %148 = load i32, ptr %147, align 4, !tbaa !14
  %149 = getelementptr inbounds i8, ptr %5, i64 4
  %150 = load i32, ptr %149, align 4, !tbaa !12
  br i1 %146, label %151, label %154

151:                                              ; preds = %142
  %152 = add i32 %148, -1
  %153 = add i32 %152, %150
  br label %159

154:                                              ; preds = %142
  %155 = add nsw i32 %150, %148
  %156 = mul nsw i32 %155, 30103
  %157 = sdiv i32 %156, 100000
  %158 = add nsw i32 %157, 1
  br label %159

159:                                              ; preds = %154, %151
  %160 = phi i32 [ %153, %151 ], [ %158, %154 ]
  %161 = getelementptr inbounds i8, ptr %5, i64 8
  %162 = load i32, ptr %161, align 8, !tbaa !13
  br i1 %146, label %163, label %165

163:                                              ; preds = %159
  %164 = add nsw i32 %162, -1
  br label %169

165:                                              ; preds = %159
  %166 = mul nsw i32 %162, 30103
  %167 = sdiv i32 %166, 100000
  %168 = add nsw i32 %167, -2
  br label %169

169:                                              ; preds = %165, %163
  %170 = phi i32 [ %164, %163 ], [ %168, %165 ]
  %171 = icmp sgt i32 %144, %160
  br i1 %171, label %241, label %172

172:                                              ; preds = %169
  %173 = icmp slt i32 %144, %170
  br i1 %173, label %174, label %177

174:                                              ; preds = %172
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %8, i8 0, i64 5604, i1 false)
  %175 = getelementptr inbounds i8, ptr %5, i64 8
  %176 = load i32, ptr %175, align 8, !tbaa !13
  br label %184

177:                                              ; preds = %172
  %178 = icmp eq i32 %145, 2
  br i1 %178, label %179, label %184

179:                                              ; preds = %177
  %180 = icmp sgt i32 %108, -1
  br i1 %180, label %181, label %182

181:                                              ; preds = %179
  call fastcc void @power(ptr noundef %8, i32 noundef 10, i32 noundef %108) #15
  br label %184

182:                                              ; preds = %179
  %183 = sub nsw i32 0, %108
  call fastcc void @power(ptr noundef %9, i32 noundef 10, i32 noundef %183) #15
  br label %184

184:                                              ; preds = %181, %182, %177, %174
  %185 = phi i32 [ %176, %174 ], [ %108, %177 ], [ 0, %182 ], [ 0, %181 ]
  %186 = call fastcc { i64, i64 } @pack(ptr noundef %8, ptr noundef %9, i32 noundef %185, i32 noundef %4, ptr noundef nonnull byval(%struct.tzrt_format) align 8 %5) #15
  %187 = extractvalue { i64, i64 } %186, 0
  %188 = extractvalue { i64, i64 } %186, 1
  %189 = zext i64 %188 to i128
  %190 = shl nuw i128 %189, 64
  %191 = zext i64 %187 to i128
  %192 = or i128 %190, %191
  %193 = getelementptr inbounds i8, ptr %5, i64 24
  %194 = load i32, ptr %193, align 8, !tbaa !17
  %195 = add nsw i32 %194, -1
  %196 = icmp eq i32 %195, 128
  %197 = zext i32 %195 to i128
  %198 = shl nsw i128 -1, %197
  %199 = xor i128 %198, -1
  %200 = select i1 %196, i128 -1, i128 %199
  %201 = and i128 %200, %192
  %202 = icmp eq i32 %145, 2
  br i1 %202, label %203, label %211

203:                                              ; preds = %184
  %204 = getelementptr inbounds i8, ptr %5, i64 20
  %205 = load i32, ptr %204, align 4, !tbaa !16
  %206 = shl nsw i32 %205, 1
  %207 = or i32 %206, 1
  %208 = sext i32 %207 to i128
  %209 = getelementptr inbounds i8, ptr %5, i64 16
  %210 = load i32, ptr %209, align 8, !tbaa !15
  br label %213

211:                                              ; preds = %184
  %212 = add nsw i32 %194, -6
  br label %213

213:                                              ; preds = %203, %211
  %214 = phi i32 [ %210, %203 ], [ %212, %211 ]
  %215 = phi i128 [ %208, %203 ], [ 30, %211 ]
  %216 = zext i32 %214 to i128
  %217 = shl i128 %215, %216
  %218 = icmp eq i128 %201, %217
  br i1 %218, label %241, label %219

219:                                              ; preds = %213
  switch i32 %194, label %220 [
    i32 8, label %225
    i32 16, label %227
    i32 32, label %229
    i32 64, label %231
    i32 128, label %232
  ]

220:                                              ; preds = %219
  %221 = icmp sgt i32 %194, 7
  br i1 %221, label %222, label %241

222:                                              ; preds = %220
  %223 = lshr i32 %194, 3
  %224 = zext i32 %223 to i64
  br label %233

225:                                              ; preds = %219
  %226 = trunc i64 %187 to i8
  store i8 %226, ptr %0, align 1, !tbaa !26
  br label %241

227:                                              ; preds = %219
  %228 = trunc i64 %187 to i16
  store i16 %228, ptr %0, align 1
  br label %241

229:                                              ; preds = %219
  %230 = trunc i64 %187 to i32
  store i32 %230, ptr %0, align 1
  br label %241

231:                                              ; preds = %219
  store i64 %187, ptr %0, align 1
  br label %241

232:                                              ; preds = %219
  store i128 %192, ptr %0, align 1
  br label %241

233:                                              ; preds = %233, %222
  %234 = phi i64 [ 0, %222 ], [ %239, %233 ]
  %235 = phi i128 [ %192, %222 ], [ %238, %233 ]
  %236 = trunc i128 %235 to i8
  %237 = getelementptr inbounds i8, ptr %0, i64 %234
  store i8 %236, ptr %237, align 1, !tbaa !26
  %238 = lshr i128 %235, 8
  %239 = add nuw nsw i64 %234, 1
  %240 = icmp eq i64 %239, %224
  br i1 %240, label %241, label %233
241:                                              ; preds = %233, %134, %99, %104, %213, %169, %96, %121, %126, %128, %130, %132, %133, %220, %225, %227, %229, %231, %232, %20, %24, %6
  %242 = phi i32 [ 0, %6 ], [ 0, %20 ], [ 0, %24 ], [ 0, %99 ], [ 0, %104 ], [ 0, %169 ], [ 0, %213 ], [ 0, %96 ], [ 1, %121 ], [ 1, %126 ], [ 1, %128 ], [ 1, %130 ], [ 1, %132 ], [ 1, %133 ], [ 1, %220 ], [ 1, %225 ], [ 1, %227 ], [ 1, %229 ], [ 1, %231 ], [ 1, %232 ], [ 1, %134 ], [ 1, %233 ]
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %9) #14
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %8) #14
  ret i32 %242
}

; Function Attrs: mustprogress nocallback nofree nounwind willreturn memory(argmem: write)
declare void @llvm.memset.p0.i64(ptr writeonly, i8, i64, i1 immarg) #9

; Function Attrs: cold noreturn nounwind memory(inaccessiblemem: write)
declare void @llvm.trap() #10

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
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %5) #14
  %48 = icmp sgt i32 %47, -1
  %49 = select i1 %48, ptr %1, ptr %0
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %5, ptr noundef nonnull readonly align 4 dereferenceable(5604) %49, i64 5604, i1 false)
  %50 = tail call i32 @llvm.abs.i32(i32 %47, i1 true)
  call fastcc void @power(ptr noundef %5, i32 noundef %2, i32 noundef %50) #15
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
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %5) #14
  br i1 %90, label %91, label %94

91:                                               ; preds = %89
  %92 = add nsw i32 %47, -1
  br label %46
93:                                               ; preds = %71, %54, %77, %57
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %5) #14
  br label %94

94:                                               ; preds = %89, %93
  %95 = getelementptr inbounds i8, ptr %4, i64 4
  %96 = icmp eq i32 %6, 0
  br label %97

97:                                               ; preds = %94, %145
  %98 = phi i32 [ %99, %145 ], [ %47, %94 ]
  %99 = add nsw i32 %98, 1
  call void @llvm.lifetime.start.p0(i64 5604, ptr nonnull %4) #14
  %100 = icmp sgt i32 %98, -2
  %101 = select i1 %100, ptr %1, ptr %0
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 4 dereferenceable(5604) %4, ptr noundef nonnull readonly align 4 dereferenceable(5604) %101, i64 5604, i1 false)
  %102 = tail call i32 @llvm.abs.i32(i32 %99, i1 true)
  call fastcc void @power(ptr noundef %4, i32 noundef %2, i32 noundef %102) #15
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
  call void @llvm.lifetime.end.p0(i64 5604, ptr nonnull %4) #14
  %147 = icmp sgt i32 %146, -1
  br i1 %147, label %97, label %148
148:                                              ; preds = %145
  ret i32 %98
}

; Function Attrs: mustprogress nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i32 @llvm.ctlz.i32(i32, i1 immarg) #11

; Function Attrs: nounwind memory(argmem: readwrite, inaccessiblemem: write)
define internal fastcc i32 @decimal_digits(ptr noundef readonly %0, i64 noundef %1, ptr noundef nonnull %2, ptr noundef %3, ptr noundef %4) unnamed_addr #2 {
  %6 = load i64, ptr %2, align 8, !tbaa !136
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
  store i64 %94, ptr %2, align 8, !tbaa !136
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

; Function Attrs: nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i32 @llvm.smin.i32(i32, i32) #12

; Function Attrs: nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i32 @llvm.scmp.i32.i32(i32, i32) #12

; Function Attrs: nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i32 @llvm.abs.i32(i32, i1 immarg) #12

; Function Attrs: nocallback nofree nosync nounwind willreturn memory(inaccessiblemem: readwrite)
declare void @llvm.experimental.noalias.scope.decl(metadata) #13

; Function Attrs: nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i32 @llvm.smax.i32(i32, i32) #12

attributes #0 = { nounwind memory(argmem: readwrite, inaccessiblemem: readwrite) "min-legal-vector-width"="0" "no-builtins" "no-trapping-math"="true" "stack-protector-buffer-size"="8" "tune-cpu"="generic" }
attributes #1 = { mustprogress nocallback nofree nosync nounwind willreturn memory(argmem: readwrite) }
attributes #2 = { nounwind memory(argmem: readwrite, inaccessiblemem: write) "min-legal-vector-width"="0" "no-builtins" "no-trapping-math"="true" "stack-protector-buffer-size"="8" "tune-cpu"="generic" }
attributes #3 = { mustprogress nocallback nofree nounwind willreturn memory(argmem: readwrite) }
attributes #4 = { nofree norecurse nosync nounwind memory(argmem: write) "min-legal-vector-width"="0" "no-builtins" "no-trapping-math"="true" "stack-protector-buffer-size"="8" "tune-cpu"="generic" }
attributes #5 = { noinline nounwind memory(argmem: readwrite, inaccessiblemem: write) "min-legal-vector-width"="0" "no-builtins" "no-trapping-math"="true" "stack-protector-buffer-size"="8" "tune-cpu"="generic" }
attributes #6 = { nounwind "min-legal-vector-width"="0" "no-builtins" "no-trapping-math"="true" "stack-protector-buffer-size"="8" "tune-cpu"="generic" }
attributes #7 = { mustprogress nofree norecurse nosync nounwind willreturn memory(argmem: readwrite) "min-legal-vector-width"="0" "no-builtins" "no-trapping-math"="true" "stack-protector-buffer-size"="8" "tune-cpu"="generic" }
attributes #8 = { noinline nounwind "min-legal-vector-width"="0" "no-builtins" "no-trapping-math"="true" "stack-protector-buffer-size"="8" "tune-cpu"="generic" }
attributes #9 = { mustprogress nocallback nofree nounwind willreturn memory(argmem: write) }
attributes #10 = { cold noreturn nounwind memory(inaccessiblemem: write) }
attributes #11 = { mustprogress nocallback nofree nosync nounwind speculatable willreturn memory(none) }
attributes #12 = { nocallback nofree nosync nounwind speculatable willreturn memory(none) }
attributes #13 = { nocallback nofree nosync nounwind willreturn memory(inaccessiblemem: readwrite) }
attributes #14 = { nounwind }
attributes #15 = { nobuiltin "no-builtins" }


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
!134 = distinct !{!134, !28, !29}
!135 = distinct !{!135, !28, !29}
!136 = !{!137, !137, i64 0}
!137 = !{!"long long", !7, i64 0}
!138 = !{!139}
!139 = distinct !{!139, !140, !"small: argument 0"}
!140 = distinct !{!140, !"small"}
!141 = !{!142}
!142 = distinct !{!142, !143, !"small: argument 0"}
!143 = distinct !{!143, !"small"}
!144 = distinct !{!144, !28, !29}
!145 = distinct !{!145, !28, !29}
!146 = distinct !{!146, !28, !29}
!147 = distinct !{!147, !28, !29}
