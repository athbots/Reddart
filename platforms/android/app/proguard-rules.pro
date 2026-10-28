# Reddart ProGuard Rules
-keep class app.reddart.ReddartEngine { *; }
-keepclassmembers class app.reddart.ReddartEngine {
    native <methods>;
}

