"""
Migration 0002 — add password column to operators_operator.

The column is nullable/blank so existing rows are not broken on upgrade.
The seed_dev_operators command sets bcrypt hashes for dev operators.
In production, provision passwords via the management command or the
Django admin change-password action, then remove blank=True in Sprint 5
once all rows have a real hash.
"""

from django.db import migrations, models


class Migration(migrations.Migration):
    dependencies = [("operators", "0001_initial")]

    operations = [
        migrations.AddField(
            model_name="operator",
            name="password",
            field=models.CharField(
                max_length=128,
                blank=True,
                default="",
                help_text=(
                    "Bcrypt hash of the operator's password.  "
                    "Set via Operator.set_password() or the admin change-password action.  "
                    "Empty string means 'no password set' (login will be refused)."
                ),
            ),
            preserve_default=False,
        ),
    ]
