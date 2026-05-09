from django.contrib import admin
from django.urls import path

admin.site.site_header = "JudicialPredict Operator Console"
admin.site.site_title = "JudicialPredict Admin"
admin.site.index_title = "Operator Dashboard"

urlpatterns = [
    path("admin/", admin.site.urls),
]
