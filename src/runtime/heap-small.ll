; Generated from heap-small.c by generate_heap.py. Do not edit.
%struct.tz_small_state = type { [16384 x ptr], ptr, i64, i32, [44 x i8], [32 x %struct.tz_small_class] }
%struct.tz_small_class = type { i32, i32, i64, i64, ptr, i64, i64, [16 x i8] }
%struct.tz_small_slot = type { ptr, i32, i32 }

@tz_small_own = internal global %struct.tz_small_state zeroinitializer, align 64
@tz_small_shared = weak hidden global ptr @tz_small_own, align 8
@tz_small_tc = internal thread_local unnamed_addr global ptr null, align 8

define internal noalias ptr @tz_small_alloc(i64 noundef %0) local_unnamed_addr nounwind noinline allockind("alloc,uninitialized") allocsize(0) "alloc-family"="tz_small" {
  %2 = icmp ugt i64 %0, 512
  br i1 %2, label %3, label %5

3:                                                ; preds = %1
  %4 = tail call ptr @malloc(i64 noundef %0) nounwind
  br label %26

5:                                                ; preds = %1
  %6 = tail call align 8 ptr @llvm.threadlocal.address.p0(ptr align 8 @tz_small_tc)
  %7 = load ptr, ptr %6, align 8
  %8 = icmp eq ptr %7, null
  br i1 %8, label %24, label %9

9:                                                ; preds = %5
  %10 = getelementptr inbounds i8, ptr %7, i64 8
  %11 = icmp ne i64 %0, 0
  %12 = sext i1 %11 to i64
  %13 = add i64 %0, %12
  %14 = lshr i64 %13, 4
  %15 = and i64 %14, 4294967295
  %16 = getelementptr inbounds [32 x %struct.tz_small_slot], ptr %10, i64 0, i64 %15
  %17 = load ptr, ptr %16, align 8
  %18 = icmp eq ptr %17, null
  br i1 %18, label %24, label %19

19:                                               ; preds = %9
  %20 = load ptr, ptr %17, align 8
  store ptr %20, ptr %16, align 8
  %21 = getelementptr inbounds i8, ptr %16, i64 8
  %22 = load i32, ptr %21, align 8
  %23 = add i32 %22, -1
  store i32 %23, ptr %21, align 8
  br label %26

24:                                               ; preds = %9, %5
  %25 = tail call fastcc ptr @tz_small_alloc_slow(i64 noundef %0)
  br label %26

26:                                               ; preds = %19, %24, %3
  %27 = phi ptr [ %4, %3 ], [ %25, %24 ], [ %17, %19 ]
  ret ptr %27
}

declare ptr @malloc(i64 noundef) local_unnamed_addr nounwind

declare nonnull ptr @llvm.threadlocal.address.p0(ptr nonnull)

define internal fastcc ptr @tz_small_alloc_slow(i64 noundef %0) unnamed_addr noinline nounwind {
  %2 = icmp ne i64 %0, 0
  %3 = sext i1 %2 to i64
  %4 = add i64 %0, %3
  %5 = lshr i64 %4, 4
  %6 = tail call align 8 ptr @llvm.threadlocal.address.p0(ptr align 8 @tz_small_tc)
  %7 = load ptr, ptr %6, align 8
  %8 = icmp eq ptr %7, null
  br i1 %8, label %9, label %38

9:                                                ; preds = %1
  %10 = tail call ptr @malloc(i64 noundef 520) nounwind
  %11 = icmp eq ptr %10, null
  br i1 %11, label %38, label %12

12:                                               ; preds = %9
  tail call void @llvm.memset.p0.i64(ptr noundef nonnull align 8 dereferenceable(520) %10, i8 0, i64 520, i1 false)
  %13 = getelementptr inbounds i8, ptr %10, i64 20
  br label %21

14:                                               ; preds = %21
  %15 = load ptr, ptr @tz_small_shared, align 8
  %16 = getelementptr inbounds i8, ptr %15, i64 131072
  %17 = ptrtoint ptr %10 to i64
  %18 = load i64, ptr %10, align 8
  %19 = cmpxchg weak ptr %16, i64 %18, i64 %17 release monotonic, align 64
  %20 = extractvalue { i64, i1 } %19, 1
  br i1 %20, label %37, label %32

21:                                               ; preds = %21, %12
  %22 = phi i64 [ 0, %12 ], [ %30, %21 ]
  %23 = trunc i64 %22 to i32
  %24 = shl i32 %23, 4
  %25 = add i32 %24, 16
  %26 = udiv i32 4096, %25
  %27 = shl i32 %26, 1
  %28 = shl i64 %22, 4
  %29 = getelementptr inbounds i8, ptr %13, i64 %28
  store i32 %27, ptr %29, align 4
  %30 = add i64 %22, 1
  %31 = icmp eq i64 %30, 32
  br i1 %31, label %14, label %21

32:                                               ; preds = %14, %32
  %33 = phi { i64, i1 } [ %35, %32 ], [ %19, %14 ]
  %34 = extractvalue { i64, i1 } %33, 0
  store i64 %34, ptr %10, align 8
  %35 = cmpxchg weak ptr %16, i64 %34, i64 %17 release monotonic, align 64
  %36 = extractvalue { i64, i1 } %35, 1
  br i1 %36, label %37, label %32

37:                                               ; preds = %32, %14
  store ptr %10, ptr %6, align 8
  br label %38

38:                                               ; preds = %37, %9, %1
  %39 = phi ptr [ %7, %1 ], [ %10, %37 ], [ null, %9 ]
  %40 = icmp eq ptr %39, null
  br i1 %40, label %144, label %41

41:                                               ; preds = %38
  %42 = load ptr, ptr @tz_small_shared, align 8
  %43 = getelementptr inbounds i8, ptr %42, i64 131136
  %44 = and i64 %5, 4294967295
  %45 = getelementptr inbounds [32 x %struct.tz_small_class], ptr %43, i64 0, i64 %44
  %46 = getelementptr inbounds i8, ptr %42, i64 131088
  %47 = load atomic i32, ptr %46 monotonic, align 16
  %48 = icmp eq i32 %47, 0
  br i1 %48, label %49, label %144

49:                                               ; preds = %41
  %50 = atomicrmw xchg ptr %45, i32 1 acquire, align 64
  %51 = icmp eq i32 %50, 0
  br i1 %51, label %52, label %144

52:                                               ; preds = %49
  %53 = getelementptr inbounds i8, ptr %45, i64 24
  %54 = load ptr, ptr %53, align 8
  %55 = icmp eq ptr %54, null
  br i1 %55, label %62, label %56

56:                                               ; preds = %52
  %57 = getelementptr inbounds i8, ptr %54, i64 8
  %58 = load ptr, ptr %57, align 8
  store ptr %58, ptr %53, align 8
  %59 = getelementptr inbounds i8, ptr %45, i64 32
  %60 = load i64, ptr %59, align 32
  %61 = add i64 %60, -1
  store i64 %61, ptr %59, align 32
  store atomic i32 0, ptr %45 release, align 64
  br label %144

62:                                               ; preds = %52
  %63 = getelementptr inbounds i8, ptr %45, i64 8
  %64 = load i64, ptr %63, align 8
  %65 = getelementptr inbounds i8, ptr %45, i64 16
  %66 = load i64, ptr %65, align 16
  %67 = icmp eq i64 %64, %66
  br i1 %67, label %68, label %122

68:                                               ; preds = %62
  %69 = tail call ptr @malloc(i64 noundef 2097152) nounwind
  %70 = icmp eq ptr %69, null
  br i1 %70, label %121, label %71

71:                                               ; preds = %68
  %72 = ptrtoint ptr %69 to i64
  %73 = add i64 %72, 1048575
  %74 = and i64 %73, -1048576
  %75 = add i64 %74, -281474975662081
  %76 = icmp ult i64 %75, -281474976710657
  br i1 %76, label %77, label %80

77:                                               ; preds = %71
  %78 = load ptr, ptr @tz_small_shared, align 8
  %79 = getelementptr inbounds i8, ptr %78, i64 131088
  store atomic i32 1, ptr %79 monotonic, align 16
  br label %120

80:                                               ; preds = %71
  %81 = lshr i64 %73, 20
  %82 = load ptr, ptr @tz_small_shared, align 8
  %83 = lshr i64 %73, 34
  %84 = getelementptr inbounds [16384 x ptr], ptr %82, i64 0, i64 %83
  %85 = load atomic i64, ptr %84 acquire, align 8
  %86 = inttoptr i64 %85 to ptr
  %87 = icmp eq i64 %85, 0
  br i1 %87, label %88, label %103

88:                                               ; preds = %80
  %89 = tail call ptr @malloc(i64 noundef 16384) nounwind
  %90 = icmp eq ptr %89, null
  br i1 %90, label %101, label %91

91:                                               ; preds = %88
  tail call void @llvm.memset.p0.i64(ptr noundef nonnull align 1 dereferenceable(16384) %89, i8 0, i64 16384, i1 false)
  %92 = ptrtoint ptr %89 to i64
  %93 = cmpxchg ptr %84, i64 %85, i64 %92 acq_rel acquire, align 8
  %94 = extractvalue { i64, i1 } %93, 1
  br i1 %94, label %95, label %98

95:                                               ; preds = %91
  %96 = getelementptr inbounds i8, ptr %82, i64 131080
  %97 = atomicrmw add ptr %96, i64 1 monotonic, align 8
  br label %101

98:                                               ; preds = %91
  %99 = extractvalue { i64, i1 } %93, 0
  %100 = inttoptr i64 %99 to ptr
  tail call void @free(ptr noundef nonnull %89) nounwind
  br label %101

101:                                              ; preds = %98, %95, %88
  %102 = phi ptr [ null, %88 ], [ %89, %95 ], [ %100, %98 ]
  br i1 %90, label %120, label %103

103:                                              ; preds = %101, %80
  %104 = phi ptr [ %86, %80 ], [ %102, %101 ]
  %105 = trunc i64 %5 to i8
  %106 = add i8 %105, 1
  %107 = and i64 %81, 16383
  %108 = getelementptr inbounds i8, ptr %104, i64 %107
  store i8 %106, ptr %108, align 1
  %109 = shl i64 %44, 4
  %110 = add i64 %109, 16
  %111 = udiv i64 4096, %110
  %112 = udiv i64 1048576, %110
  %113 = urem i64 %112, %111
  %114 = sub i64 %112, %113
  store i64 %74, ptr %63, align 8
  %115 = mul i64 %114, %110
  %116 = add i64 %115, %74
  store i64 %116, ptr %65, align 16
  %117 = getelementptr inbounds i8, ptr %45, i64 4
  %118 = load i32, ptr %117, align 4
  %119 = add i32 %118, 1
  store i32 %119, ptr %117, align 4
  br label %122

120:                                              ; preds = %101, %77
  tail call void @free(ptr noundef nonnull %69) nounwind
  br label %121

121:                                              ; preds = %120, %68
  store atomic i32 0, ptr %45 release, align 64
  br label %144

122:                                              ; preds = %103, %62
  %123 = shl i64 %44, 4
  %124 = add i64 %123, 16
  %125 = udiv i64 4096, %124
  %126 = trunc i64 %125 to i32
  %127 = load i64, ptr %63, align 8
  %128 = inttoptr i64 %127 to ptr
  %129 = mul i64 %125, %124
  %130 = add i64 %127, %129
  store i64 %130, ptr %63, align 8
  %131 = getelementptr inbounds i8, ptr %45, i64 40
  %132 = load i64, ptr %131, align 8
  %133 = add i64 %132, %125
  store i64 %133, ptr %131, align 8
  store atomic i32 0, ptr %45 release, align 64
  %134 = and i64 %4, 68719474688
  %135 = icmp eq i64 %134, 0
  br i1 %135, label %138, label %136

136:                                              ; preds = %138, %122
  %137 = phi ptr [ %128, %122 ], [ %141, %138 ]
  store ptr null, ptr %137, align 8
  br label %144

138:                                              ; preds = %122, %138
  %139 = phi i32 [ %142, %138 ], [ 1, %122 ]
  %140 = phi ptr [ %141, %138 ], [ %128, %122 ]
  %141 = getelementptr inbounds i8, ptr %140, i64 %124
  store ptr %141, ptr %140, align 8
  %142 = add i32 %139, 1
  %143 = icmp eq i32 %142, %126
  br i1 %143, label %136, label %138

144:                                              ; preds = %136, %121, %56, %49, %41, %38
  %145 = phi ptr [ null, %38 ], [ null, %49 ], [ null, %41 ], [ %54, %56 ], [ %128, %136 ], [ null, %121 ]
  %146 = icmp eq ptr %145, null
  br i1 %146, label %147, label %149

147:                                              ; preds = %144
  %148 = tail call ptr @malloc(i64 noundef %0) nounwind
  br label %159

149:                                              ; preds = %144
  %150 = getelementptr inbounds i8, ptr %39, i64 8
  %151 = and i64 %5, 4294967295
  %152 = getelementptr inbounds [32 x %struct.tz_small_slot], ptr %150, i64 0, i64 %151
  %153 = load ptr, ptr %145, align 8
  store ptr %153, ptr %152, align 8
  %154 = getelementptr inbounds i8, ptr %152, i64 12
  %155 = load i32, ptr %154, align 4
  %156 = lshr i32 %155, 1
  %157 = add i32 %156, -1
  %158 = getelementptr inbounds i8, ptr %152, i64 8
  store i32 %157, ptr %158, align 8
  br label %159

159:                                              ; preds = %149, %147
  %160 = phi ptr [ %145, %149 ], [ %148, %147 ]
  ret ptr %160
}

define internal void @tz_small_free(ptr noundef allocptr %0) local_unnamed_addr nounwind noinline allockind("free") "alloc-family"="tz_small" {
  %2 = ptrtoint ptr %0 to i64
  %3 = icmp ult ptr %0, inttoptr (i64 281474976710656 to ptr)
  br i1 %3, label %4, label %17

4:                                                ; preds = %1
  %5 = load ptr, ptr @tz_small_shared, align 8
  %6 = lshr i64 %2, 34
  %7 = getelementptr inbounds [16384 x ptr], ptr %5, i64 0, i64 %6
  %8 = load atomic i64, ptr %7 acquire, align 8
  %9 = icmp eq i64 %8, 0
  br i1 %9, label %17, label %10

10:                                               ; preds = %4
  %11 = inttoptr i64 %8 to ptr
  %12 = lshr i64 %2, 20
  %13 = and i64 %12, 16383
  %14 = getelementptr inbounds i8, ptr %11, i64 %13
  %15 = load i8, ptr %14, align 1
  %16 = zext i8 %15 to i32
  br label %17

17:                                               ; preds = %1, %4, %10
  %18 = phi i32 [ 0, %1 ], [ %16, %10 ], [ 0, %4 ]
  %19 = icmp eq i32 %18, 0
  br i1 %19, label %20, label %21

20:                                               ; preds = %17
  tail call void @free(ptr noundef %0) nounwind
  br label %40

21:                                               ; preds = %17
  %22 = tail call align 8 ptr @llvm.threadlocal.address.p0(ptr align 8 @tz_small_tc)
  %23 = load ptr, ptr %22, align 8
  %24 = icmp eq ptr %23, null
  br i1 %24, label %25, label %27

25:                                               ; preds = %21
  %26 = add i32 %18, -1
  tail call fastcc void @tz_small_free_slow(ptr noundef %0, i32 noundef %26)
  br label %40

27:                                               ; preds = %21
  %28 = getelementptr inbounds i8, ptr %23, i64 8
  %29 = add i32 %18, -1
  %30 = zext i32 %29 to i64
  %31 = getelementptr inbounds [32 x %struct.tz_small_slot], ptr %28, i64 0, i64 %30
  %32 = load ptr, ptr %31, align 8
  store ptr %32, ptr %0, align 8
  store ptr %0, ptr %31, align 8
  %33 = getelementptr inbounds i8, ptr %31, i64 8
  %34 = load i32, ptr %33, align 8
  %35 = add i32 %34, 1
  store i32 %35, ptr %33, align 8
  %36 = getelementptr inbounds i8, ptr %31, i64 12
  %37 = load i32, ptr %36, align 4
  %38 = icmp ugt i32 %35, %37
  br i1 %38, label %39, label %40

39:                                               ; preds = %27
  tail call fastcc void @tz_small_flush(ptr noundef %31, i32 noundef %29)
  br label %40

40:                                               ; preds = %25, %39, %27, %20
  ret void
}

declare void @free(ptr noundef) local_unnamed_addr nounwind

define internal fastcc void @tz_small_free_slow(ptr noundef %0, i32 noundef %1) unnamed_addr noinline nounwind {
  %3 = tail call ptr @malloc(i64 noundef 520) nounwind
  %4 = icmp eq ptr %3, null
  br i1 %4, label %32, label %5

5:                                                ; preds = %2
  tail call void @llvm.memset.p0.i64(ptr noundef nonnull align 8 dereferenceable(520) %3, i8 0, i64 520, i1 false)
  %6 = getelementptr inbounds i8, ptr %3, i64 20
  br label %14

7:                                                ; preds = %14
  %8 = load ptr, ptr @tz_small_shared, align 8
  %9 = getelementptr inbounds i8, ptr %8, i64 131072
  %10 = ptrtoint ptr %3 to i64
  %11 = load i64, ptr %3, align 8
  %12 = cmpxchg weak ptr %9, i64 %11, i64 %10 release monotonic, align 64
  %13 = extractvalue { i64, i1 } %12, 1
  br i1 %13, label %30, label %25

14:                                               ; preds = %14, %5
  %15 = phi i64 [ 0, %5 ], [ %23, %14 ]
  %16 = trunc i64 %15 to i32
  %17 = shl i32 %16, 4
  %18 = add i32 %17, 16
  %19 = udiv i32 4096, %18
  %20 = shl i32 %19, 1
  %21 = shl i64 %15, 4
  %22 = getelementptr inbounds i8, ptr %6, i64 %21
  store i32 %20, ptr %22, align 4
  %23 = add i64 %15, 1
  %24 = icmp eq i64 %23, 32
  br i1 %24, label %7, label %14

25:                                               ; preds = %7, %25
  %26 = phi { i64, i1 } [ %28, %25 ], [ %12, %7 ]
  %27 = extractvalue { i64, i1 } %26, 0
  store i64 %27, ptr %3, align 8
  %28 = cmpxchg weak ptr %9, i64 %27, i64 %10 release monotonic, align 64
  %29 = extractvalue { i64, i1 } %28, 1
  br i1 %29, label %30, label %25

30:                                               ; preds = %25, %7
  %31 = tail call align 8 ptr @llvm.threadlocal.address.p0(ptr align 8 @tz_small_tc)
  store ptr %3, ptr %31, align 8
  br label %32

32:                                               ; preds = %2, %30
  %33 = phi ptr [ %3, %30 ], [ null, %2 ]
  %34 = icmp eq ptr %33, null
  br i1 %34, label %40, label %35

35:                                               ; preds = %32
  %36 = getelementptr inbounds i8, ptr %33, i64 8
  %37 = zext i32 %1 to i64
  %38 = getelementptr inbounds [32 x %struct.tz_small_slot], ptr %36, i64 0, i64 %37
  store ptr null, ptr %0, align 8
  store ptr %0, ptr %38, align 8
  %39 = getelementptr inbounds i8, ptr %38, i64 8
  store i32 1, ptr %39, align 8
  br label %40

40:                                               ; preds = %32, %35
  ret void
}

define internal fastcc void @tz_small_flush(ptr noundef nonnull %0, i32 noundef %1) unnamed_addr noinline nounwind {
  %3 = load ptr, ptr @tz_small_shared, align 8
  %4 = getelementptr inbounds i8, ptr %3, i64 131136
  %5 = zext i32 %1 to i64
  %6 = getelementptr inbounds [32 x %struct.tz_small_class], ptr %4, i64 0, i64 %5
  %7 = atomicrmw xchg ptr %6, i32 1 acquire, align 64
  %8 = icmp eq i32 %7, 0
  br i1 %8, label %9, label %33

9:                                                ; preds = %2
  %10 = getelementptr inbounds i8, ptr %0, i64 12
  %11 = load i32, ptr %10, align 4
  %12 = lshr i32 %11, 1
  %13 = load ptr, ptr %0, align 8
  %14 = icmp ugt i32 %11, 3
  br i1 %14, label %27, label %15

15:                                               ; preds = %27, %9
  %16 = phi ptr [ %13, %9 ], [ %30, %27 ]
  %17 = load ptr, ptr %16, align 8
  store ptr %17, ptr %0, align 8
  %18 = getelementptr inbounds i8, ptr %0, i64 8
  %19 = load i32, ptr %18, align 8
  %20 = sub i32 %19, %12
  store i32 %20, ptr %18, align 8
  store ptr null, ptr %16, align 8
  %21 = getelementptr inbounds i8, ptr %6, i64 24
  %22 = load ptr, ptr %21, align 8
  %23 = getelementptr inbounds i8, ptr %13, i64 8
  store ptr %22, ptr %23, align 8
  store ptr %13, ptr %21, align 8
  %24 = getelementptr inbounds i8, ptr %6, i64 32
  %25 = load i64, ptr %24, align 32
  %26 = add i64 %25, 1
  store i64 %26, ptr %24, align 32
  store atomic i32 0, ptr %6 release, align 64
  br label %33

27:                                               ; preds = %9, %27
  %28 = phi i32 [ %31, %27 ], [ 1, %9 ]
  %29 = phi ptr [ %30, %27 ], [ %13, %9 ]
  %30 = load ptr, ptr %29, align 8
  %31 = add i32 %28, 1
  %32 = icmp eq i32 %31, %12
  br i1 %32, label %15, label %27

33:                                               ; preds = %2, %15
  ret void
}

define internal ptr @tz_small_realloc(ptr noundef %0, i64 noundef %1) local_unnamed_addr nounwind {
  %3 = icmp eq ptr %0, null
  br i1 %3, label %4, label %29

4:                                                ; preds = %2
  %5 = icmp ugt i64 %1, 512
  br i1 %5, label %6, label %8

6:                                                ; preds = %4
  %7 = tail call ptr @malloc(i64 noundef %1) nounwind
  br label %165

8:                                                ; preds = %4
  %9 = tail call align 8 ptr @llvm.threadlocal.address.p0(ptr align 8 @tz_small_tc)
  %10 = load ptr, ptr %9, align 8
  %11 = icmp eq ptr %10, null
  br i1 %11, label %27, label %12

12:                                               ; preds = %8
  %13 = getelementptr inbounds i8, ptr %10, i64 8
  %14 = icmp ne i64 %1, 0
  %15 = sext i1 %14 to i64
  %16 = add i64 %1, %15
  %17 = lshr i64 %16, 4
  %18 = and i64 %17, 4294967295
  %19 = getelementptr inbounds [32 x %struct.tz_small_slot], ptr %13, i64 0, i64 %18
  %20 = load ptr, ptr %19, align 8
  %21 = icmp eq ptr %20, null
  br i1 %21, label %27, label %22

22:                                               ; preds = %12
  %23 = load ptr, ptr %20, align 8
  store ptr %23, ptr %19, align 8
  %24 = getelementptr inbounds i8, ptr %19, i64 8
  %25 = load i32, ptr %24, align 8
  %26 = add i32 %25, -1
  store i32 %26, ptr %24, align 8
  br label %165

27:                                               ; preds = %12, %8
  %28 = tail call fastcc ptr @tz_small_alloc_slow(i64 noundef %1)
  br label %165

29:                                               ; preds = %2
  %30 = icmp eq i64 %1, 0
  %31 = ptrtoint ptr %0 to i64
  %32 = icmp ult ptr %0, inttoptr (i64 281474976710656 to ptr)
  br i1 %30, label %33, label %70

33:                                               ; preds = %29
  br i1 %32, label %34, label %47

34:                                               ; preds = %33
  %35 = load ptr, ptr @tz_small_shared, align 8
  %36 = lshr i64 %31, 34
  %37 = getelementptr inbounds [16384 x ptr], ptr %35, i64 0, i64 %36
  %38 = load atomic i64, ptr %37 acquire, align 8
  %39 = icmp eq i64 %38, 0
  br i1 %39, label %47, label %40

40:                                               ; preds = %34
  %41 = inttoptr i64 %38 to ptr
  %42 = lshr i64 %31, 20
  %43 = and i64 %42, 16383
  %44 = getelementptr inbounds i8, ptr %41, i64 %43
  %45 = load i8, ptr %44, align 1
  %46 = zext i8 %45 to i32
  br label %47

47:                                               ; preds = %40, %34, %33
  %48 = phi i32 [ 0, %33 ], [ %46, %40 ], [ 0, %34 ]
  %49 = icmp eq i32 %48, 0
  br i1 %49, label %50, label %51

50:                                               ; preds = %47
  tail call void @free(ptr noundef nonnull %0) nounwind
  br label %165

51:                                               ; preds = %47
  %52 = tail call align 8 ptr @llvm.threadlocal.address.p0(ptr align 8 @tz_small_tc)
  %53 = load ptr, ptr %52, align 8
  %54 = icmp eq ptr %53, null
  br i1 %54, label %55, label %57

55:                                               ; preds = %51
  %56 = add i32 %48, -1
  tail call fastcc void @tz_small_free_slow(ptr noundef nonnull %0, i32 noundef %56)
  br label %165

57:                                               ; preds = %51
  %58 = getelementptr inbounds i8, ptr %53, i64 8
  %59 = add i32 %48, -1
  %60 = zext i32 %59 to i64
  %61 = getelementptr inbounds [32 x %struct.tz_small_slot], ptr %58, i64 0, i64 %60
  %62 = load ptr, ptr %61, align 8
  store ptr %62, ptr %0, align 8
  store ptr %0, ptr %61, align 8
  %63 = getelementptr inbounds i8, ptr %61, i64 8
  %64 = load i32, ptr %63, align 8
  %65 = add i32 %64, 1
  store i32 %65, ptr %63, align 8
  %66 = getelementptr inbounds i8, ptr %61, i64 12
  %67 = load i32, ptr %66, align 4
  %68 = icmp ugt i32 %65, %67
  br i1 %68, label %69, label %165

69:                                               ; preds = %57
  tail call fastcc void @tz_small_flush(ptr noundef %61, i32 noundef %59)
  br label %165

70:                                               ; preds = %29
  br i1 %32, label %71, label %84

71:                                               ; preds = %70
  %72 = load ptr, ptr @tz_small_shared, align 8
  %73 = lshr i64 %31, 34
  %74 = getelementptr inbounds [16384 x ptr], ptr %72, i64 0, i64 %73
  %75 = load atomic i64, ptr %74 acquire, align 8
  %76 = icmp eq i64 %75, 0
  br i1 %76, label %84, label %77

77:                                               ; preds = %71
  %78 = inttoptr i64 %75 to ptr
  %79 = lshr i64 %31, 20
  %80 = and i64 %79, 16383
  %81 = getelementptr inbounds i8, ptr %78, i64 %80
  %82 = load i8, ptr %81, align 1
  %83 = zext i8 %82 to i32
  br label %84

84:                                               ; preds = %70, %71, %77
  %85 = phi i32 [ 0, %70 ], [ %83, %77 ], [ 0, %71 ]
  %86 = icmp eq i32 %85, 0
  br i1 %86, label %87, label %89

87:                                               ; preds = %84
  %88 = tail call ptr @realloc(ptr noundef nonnull %0, i64 noundef %1) nounwind
  br label %165

89:                                               ; preds = %84
  %90 = icmp ult i64 %1, 513
  br i1 %90, label %91, label %97

91:                                               ; preds = %89
  %92 = add i64 %1, 68719476735
  %93 = lshr i64 %92, 4
  %94 = trunc i64 %93 to i32
  %95 = add i32 %85, -1
  %96 = icmp eq i32 %95, %94
  br i1 %96, label %165, label %97

97:                                               ; preds = %91, %89
  %98 = icmp ugt i64 %1, 512
  br i1 %98, label %99, label %101

99:                                               ; preds = %97
  %100 = tail call ptr @malloc(i64 noundef %1) nounwind
  br label %120

101:                                              ; preds = %97
  %102 = tail call align 8 ptr @llvm.threadlocal.address.p0(ptr align 8 @tz_small_tc)
  %103 = load ptr, ptr %102, align 8
  %104 = icmp eq ptr %103, null
  br i1 %104, label %118, label %105

105:                                              ; preds = %101
  %106 = getelementptr inbounds i8, ptr %103, i64 8
  %107 = add i64 %1, 68719476735
  %108 = lshr i64 %107, 4
  %109 = and i64 %108, 4294967295
  %110 = getelementptr inbounds [32 x %struct.tz_small_slot], ptr %106, i64 0, i64 %109
  %111 = load ptr, ptr %110, align 8
  %112 = icmp eq ptr %111, null
  br i1 %112, label %118, label %113

113:                                              ; preds = %105
  %114 = load ptr, ptr %111, align 8
  store ptr %114, ptr %110, align 8
  %115 = getelementptr inbounds i8, ptr %110, i64 8
  %116 = load i32, ptr %115, align 8
  %117 = add i32 %116, -1
  store i32 %117, ptr %115, align 8
  br label %120

118:                                              ; preds = %105, %101
  %119 = tail call fastcc ptr @tz_small_alloc_slow(i64 noundef %1)
  br label %120

120:                                              ; preds = %99, %113, %118
  %121 = phi ptr [ %100, %99 ], [ %119, %118 ], [ %111, %113 ]
  %122 = icmp eq ptr %121, null
  br i1 %122, label %165, label %123

123:                                              ; preds = %120
  %124 = add i32 %85, -1
  %125 = zext i32 %124 to i64
  %126 = shl i64 %125, 4
  %127 = add i64 %126, 16
  %128 = tail call i64 @llvm.umin.i64(i64 %127, i64 %1)
  tail call void @llvm.memcpy.p0.p0.i64(ptr nonnull align 1 %121, ptr nonnull align 1 %0, i64 %128, i1 false)
  br i1 %32, label %129, label %142

129:                                              ; preds = %123
  %130 = load ptr, ptr @tz_small_shared, align 8
  %131 = lshr i64 %31, 34
  %132 = getelementptr inbounds [16384 x ptr], ptr %130, i64 0, i64 %131
  %133 = load atomic i64, ptr %132 acquire, align 8
  %134 = icmp eq i64 %133, 0
  br i1 %134, label %142, label %135

135:                                              ; preds = %129
  %136 = inttoptr i64 %133 to ptr
  %137 = lshr i64 %31, 20
  %138 = and i64 %137, 16383
  %139 = getelementptr inbounds i8, ptr %136, i64 %138
  %140 = load i8, ptr %139, align 1
  %141 = zext i8 %140 to i32
  br label %142

142:                                              ; preds = %135, %129, %123
  %143 = phi i32 [ 0, %123 ], [ %141, %135 ], [ 0, %129 ]
  %144 = icmp eq i32 %143, 0
  br i1 %144, label %145, label %146

145:                                              ; preds = %142
  tail call void @free(ptr noundef nonnull %0) nounwind
  br label %165

146:                                              ; preds = %142
  %147 = tail call align 8 ptr @llvm.threadlocal.address.p0(ptr align 8 @tz_small_tc)
  %148 = load ptr, ptr %147, align 8
  %149 = icmp eq ptr %148, null
  br i1 %149, label %150, label %152

150:                                              ; preds = %146
  %151 = add i32 %143, -1
  tail call fastcc void @tz_small_free_slow(ptr noundef nonnull %0, i32 noundef %151)
  br label %165

152:                                              ; preds = %146
  %153 = getelementptr inbounds i8, ptr %148, i64 8
  %154 = add i32 %143, -1
  %155 = zext i32 %154 to i64
  %156 = getelementptr inbounds [32 x %struct.tz_small_slot], ptr %153, i64 0, i64 %155
  %157 = load ptr, ptr %156, align 8
  store ptr %157, ptr %0, align 8
  store ptr %0, ptr %156, align 8
  %158 = getelementptr inbounds i8, ptr %156, i64 8
  %159 = load i32, ptr %158, align 8
  %160 = add i32 %159, 1
  store i32 %160, ptr %158, align 8
  %161 = getelementptr inbounds i8, ptr %156, i64 12
  %162 = load i32, ptr %161, align 4
  %163 = icmp ugt i32 %160, %162
  br i1 %163, label %164, label %165

164:                                              ; preds = %152
  tail call fastcc void @tz_small_flush(ptr noundef %156, i32 noundef %154)
  br label %165

165:                                              ; preds = %164, %152, %150, %145, %69, %57, %55, %50, %27, %22, %6, %87, %91, %120
  %166 = phi ptr [ %88, %87 ], [ %0, %91 ], [ null, %120 ], [ %7, %6 ], [ %28, %27 ], [ %20, %22 ], [ null, %50 ], [ null, %55 ], [ null, %57 ], [ null, %69 ], [ %121, %145 ], [ %121, %150 ], [ %121, %152 ], [ %121, %164 ]
  ret ptr %166
}

declare ptr @realloc(ptr noundef, i64 noundef) local_unnamed_addr nounwind

declare void @llvm.memcpy.p0.p0.i64(ptr noalias writeonly, ptr noalias readonly, i64, i1 immarg)

define weak hidden void @tz_small_stats(ptr noundef writeonly %0) local_unnamed_addr nounwind {
  %2 = load ptr, ptr @tz_small_shared, align 8
  %3 = getelementptr inbounds i8, ptr %2, i64 131136
  br label %10

4:                                                ; preds = %19
  %5 = getelementptr inbounds i8, ptr %2, i64 131072
  %6 = load atomic i64, ptr %5 acquire, align 64
  %7 = icmp eq i64 %6, 0
  br i1 %7, label %38, label %8

8:                                                ; preds = %4
  %9 = inttoptr i64 %6 to ptr
  br label %52

10:                                               ; preds = %1, %19
  %11 = phi i64 [ 0, %1 ], [ %36, %19 ]
  %12 = phi i64 [ 0, %1 ], [ %25, %19 ]
  %13 = phi i64 [ 0, %1 ], [ %31, %19 ]
  %14 = phi i64 [ 0, %1 ], [ %35, %19 ]
  %15 = getelementptr inbounds [32 x %struct.tz_small_class], ptr %3, i64 0, i64 %11
  br label %16

16:                                               ; preds = %16, %10
  %17 = atomicrmw xchg ptr %15, i32 1 acquire, align 64
  %18 = icmp eq i32 %17, 0
  br i1 %18, label %19, label %16

19:                                               ; preds = %16
  %20 = shl i64 %11, 4
  %21 = add i64 %20, 16
  %22 = getelementptr inbounds i8, ptr %15, i64 40
  %23 = load i64, ptr %22, align 8
  %24 = mul i64 %23, %21
  %25 = add i64 %24, %12
  %26 = getelementptr inbounds i8, ptr %15, i64 32
  %27 = load i64, ptr %26, align 32
  %28 = udiv i64 4096, %21
  %29 = mul i64 %27, %21
  %30 = mul i64 %29, %28
  %31 = add i64 %30, %13
  %32 = getelementptr inbounds i8, ptr %15, i64 4
  %33 = load i32, ptr %32, align 4
  %34 = zext i32 %33 to i64
  %35 = add i64 %14, %34
  store atomic i32 0, ptr %15 release, align 64
  %36 = add i64 %11, 1
  %37 = icmp eq i64 %36, 32
  br i1 %37, label %4, label %10

38:                                               ; preds = %57, %4
  %39 = phi i64 [ 0, %4 ], [ %58, %57 ]
  %40 = phi i64 [ %31, %4 ], [ %70, %57 ]
  store i64 %25, ptr %0, align 8
  %41 = getelementptr inbounds i8, ptr %0, i64 8
  store i64 %40, ptr %41, align 8
  %42 = getelementptr inbounds i8, ptr %0, i64 16
  store i64 %35, ptr %42, align 8
  %43 = getelementptr inbounds i8, ptr %0, i64 24
  store i64 %39, ptr %43, align 8
  %44 = shl i64 %35, 21
  %45 = getelementptr inbounds i8, ptr %2, i64 131080
  %46 = load atomic i64, ptr %45 monotonic, align 8
  %47 = shl i64 %46, 14
  %48 = mul i64 %39, 520
  %49 = add i64 %48, %44
  %50 = add i64 %49, %47
  %51 = getelementptr inbounds i8, ptr %0, i64 32
  store i64 %50, ptr %51, align 8
  ret void

52:                                               ; preds = %8, %57
  %53 = phi i64 [ %70, %57 ], [ %31, %8 ]
  %54 = phi ptr [ %59, %57 ], [ %9, %8 ]
  %55 = phi i64 [ %58, %57 ], [ 0, %8 ]
  %56 = getelementptr inbounds i8, ptr %54, i64 16
  br label %61

57:                                               ; preds = %61
  %58 = add i64 %55, 1
  %59 = load ptr, ptr %54, align 8
  %60 = icmp eq ptr %59, null
  br i1 %60, label %38, label %52

61:                                               ; preds = %52, %61
  %62 = phi i64 [ 0, %52 ], [ %71, %61 ]
  %63 = phi i64 [ %53, %52 ], [ %70, %61 ]
  %64 = shl i64 %62, 4
  %65 = getelementptr inbounds i8, ptr %56, i64 %64
  %66 = load i32, ptr %65, align 8
  %67 = zext i32 %66 to i64
  %68 = add i64 %64, 16
  %69 = mul i64 %68, %67
  %70 = add i64 %69, %63
  %71 = add i64 %62, 1
  %72 = icmp eq i64 %71, 32
  br i1 %72, label %57, label %61
}

declare void @llvm.memset.p0.i64(ptr writeonly, i8, i64, i1 immarg)

declare i64 @llvm.umin.i64(i64, i64)
